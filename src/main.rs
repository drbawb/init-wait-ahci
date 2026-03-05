use std::{fmt, fs, io};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const WAIT_NUM_DEVICES: u8 = 4;
const WAIT_ATA_DEVICES: u8 = 24;

const PCI_BUS_PATH: &str = "/sys/bus/pci/devices";

const PCI_DEV_BID: &str = "0000:67:00.0";
const PCI_DEV_VID: &str = "0x1b21";
const PCI_DEV_DID: &str = "0x1164";

#[derive(Debug)]
struct TaskRegister {
    devices_found: u8,
}

#[derive(Debug)]
enum CmdErr {
    Io(io::Error),
    Logic(String),
}

impl std::error::Error for CmdErr {}

impl From<io::Error> for CmdErr {
    fn from(err: io::Error) -> CmdErr { CmdErr::Io(err) }
}

impl From<&str> for CmdErr {
    fn from(err: &str) -> CmdErr { CmdErr::Logic(err.into()) }
}

impl From<String> for CmdErr {
    fn from(err: String) -> CmdErr { CmdErr::Logic(err) }
}

impl fmt::Display for CmdErr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), fmt::Error> {
        Ok(match self {
            Self::Io(err) => write!(f, "i/o error: {err:?}")?,
            Self::Logic(err) => write!(f, "program error: {err:?}")?,
        })
    }
}

fn main() {
    let dev_path = format!("{PCI_BUS_PATH}/{PCI_DEV_BID}");
    let dev_id = format!("{PCI_DEV_VID}:{PCI_DEV_DID}");
    println!("looking for ASMedia controller {dev_id} at {dev_path}");

    let pci_probe_time = Instant::now();

    // probe for the AHCI controller on the other side of the TBT3 tunnel ...
    'probe: loop {
        let vendor_str = fs::read_to_string(format!("{dev_path}/vendor")).unwrap_or("0x0000".to_string());
        let device_str = fs::read_to_string(format!("{dev_path}/device")).unwrap_or("0x0000".to_string());

        if vendor_str == "0x0000" && device_str == "0x0000" {
            if pci_probe_time.elapsed() > Duration::from_secs(10) {
                eprintln!("timeout elapsed ... giving up on probing");
                std::process::exit(0x01);
            }

            thread::sleep(Duration::from_millis(100));
            continue 'probe;
        }

        if vendor_str.trim_end() != PCI_DEV_VID {
            eprintln!("vendor ID {vendor_str} did not match expected {PCI_DEV_VID}");
            std::process::exit(0x01);
        }
        
        if device_str.trim_end() != PCI_DEV_DID {
            eprintln!("device ID {device_str} did not match expected {PCI_DEV_DID}");
            std::process::exit(0x01);
        }

        break 'probe; // we found the device we are looking for ...
    }

    // since it is an AHCI controller we expect it to have one or more ataN
    // devices in its device-tree, collect them for later discovery ...
    println!("found ASMedia controller, enumerating ATA paths ...");
    
    let dir_tree = fs::read_dir(format!("{dev_path}")).inspect_err(|e| {
        eprintln!("io error reading {dev_path}: {e}");
        std::process::exit(0x10);
    }).unwrap();

    let paths = enumerate_ata_paths(dir_tree).inspect_err(|err| {
        eprintln!("io error enumerating ata* paths: {err:?}");
        std::process::exit(0x10);
    }).unwrap();

    println!("discovered {} paths", paths.len());

    // kick off some threads which monitor each ata device independently
    let mut any_error = false;
    let mut handles = Vec::with_capacity(paths.len());
    let start_time = Instant::now();
    let thread_state = Arc::new(Mutex::new(TaskRegister { devices_found: 0 }));

    for path in paths {
        handles.push(wait_for_ata_dev(start_time, path, thread_state.clone()));
    }

    for handle in handles { 
        if let Err(e) = handle.join().expect("thread panicked") {
            any_error = true; eprintln!("thread encountered an error: {e:?}");
        }
    }

    if any_error { std::process::exit(0x10); }

    // check the results from our workers if there were no unexpected errors ...
    let task_regs = thread_state.lock().expect("could not lock task register state");

    if task_regs.devices_found != WAIT_NUM_DEVICES {
        eprintln!("found [{}] devices expected [{WAIT_NUM_DEVICES}]", task_regs.devices_found);
        std::process::exit(0x01);
    }

    println!("all devices found :D");
    std::process::exit(0x00);
}

fn wait_for_ata_dev(started_at: Instant, path: String, regs: Arc<Mutex<TaskRegister>>) -> JoinHandle<Result<(), CmdErr>> {
    thread::spawn(move || {
        let ata_no: u8 = path
            .strip_prefix("ata").ok_or(CmdErr::from("started thread with non ata* path?"))?
            .parse().map_err(|_| CmdErr::from("ata* path does not end with integer?"))?;

        let scsi_no = ata_no.checked_sub(1)
            .ok_or(CmdErr::from(format!("integer underflow: did not expect ata device [{ata_no}]")))?;

        let host_path = format!("{PCI_BUS_PATH}/{PCI_DEV_BID}/ata{ata_no}/host{scsi_no}/target{scsi_no}:0:0/{scsi_no}:0:0:0");
        println!("started thread for {host_path}");

        'task: loop {
            if started_at.elapsed() > Duration::from_secs(30) {
                return Err(CmdErr::from(format!("timeout exceeded for: {path}")))
            }

            let mut task_reg = regs.lock().map_err(|_| CmdErr::from("failed to lock task register"))?;
            if task_reg.devices_found >= WAIT_NUM_DEVICES { break 'task } // exit early if other threads beat us ...

            if fs::exists(host_path.as_str()).map_err(|_| CmdErr::from("i/o error reading host path"))? {
                let elapsed_ms = started_at.elapsed().as_millis();
                task_reg.devices_found += 1;
                println!("found device at [{host_path}] in {elapsed_ms}ms");
                break 'task // ... or exit early when device is found
            }

            drop(task_reg); // lock is otherwise held for scope of loop
            thread::sleep(Duration::from_millis(100));
        }

        Ok(()) // exit successfully ...
    })
}

fn enumerate_ata_paths(dir_tree: fs::ReadDir) -> Result<Vec<String>, CmdErr> {
    let mut paths = Vec::with_capacity(WAIT_ATA_DEVICES as usize);

    for ent in dir_tree {
        let ent = ent?; // assume we are able to read this dnode
        let info = ent.file_type()?;

        if !info.is_dir() { continue; } // looking only for ata* directories

        let path_name = ent.file_name().to_string_lossy().into_owned();
        if !path_name.starts_with("ata") { continue; }

        paths.push(path_name); // found an ATA path
    }

    Ok(paths)
}
