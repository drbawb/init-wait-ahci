use std::fs;
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const PCI_BUS_PATH: &'static str = "/sys/bus/pci/devices";

const PCI_DEV_BID: &'static str = "0000:67:00.0";
const PCI_DEV_VID: &'static str = "0x1b21";
const PCI_DEV_DID: &'static str = "0x1164";

struct TaskRegister {
    devices_found: i8,
}



fn main() {
    let dev_path = format!("{PCI_BUS_PATH}/{PCI_DEV_BID}");
    let dev_id = format!("{PCI_DEV_VID}:{PCI_DEV_DID}");
    println!("looking for ASMedia controller {dev_id} at {dev_path}");

    let pci_probe_time = Instant::now();

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

    println!("found ASMedia controller, enumerating ATA paths ...");
    
    let dir_tree = fs::read_dir(format!("{dev_path}")).inspect_err(|e| {
        eprintln!("io error reading {dev_path}: {e}");
        std::process::exit(0x10);
    }).unwrap();

    let paths = enumerate_ata_paths(dir_tree);
    println!("discovered {} paths", paths.len());

    let start_time = Instant::now();
    let thread_state = Arc::new(RwLock::new(TaskRegister { devices_found: 0 }));
    let mut handles = Vec::with_capacity(paths.len());


    for path in paths {
        handles.push(wait_for_ata_dev(start_time, path, thread_state.clone()));
    }

    for handle in handles { handle.join().expect("thread panicked"); }

    let task_regs = thread_state.read().expect("could not lock task register state");
    if task_regs.devices_found != 4 {
        eprintln!("found [{}] devices expected [4]", task_regs.devices_found);
        std::process::exit(0x01);
    }

    println!("all devices found :D");
    std::process::exit(0x00);
}

fn wait_for_ata_dev(started_at: Instant, path: String, regs: Arc<RwLock<TaskRegister>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let ata_no: i8 = path
            .strip_prefix("ata").expect("started thread with non ata* path?")
            .parse().expect("ata* path does not end with integer?");

        let scsi_no = ata_no - 1;
        let host_path = format!("{PCI_BUS_PATH}/{PCI_DEV_BID}/ata{ata_no}/host{scsi_no}/target{scsi_no}:0:0/{scsi_no}:0:0:0");
        println!("started thread for {host_path}");

        'task: loop {
            if started_at.elapsed() > Duration::from_secs(30) {
                eprintln!("timeout reached for {host_path}");
                break 'task; // exit early on timeout ...
            }

            let mut task_reg = regs.write().expect("failed to lock task register");
            if task_reg.devices_found >= 4 { break 'task } // exit early if other threads beat us ...

            if fs::exists(host_path.as_str()).expect("i/o error reading host path") {
                let elapsed_ms = started_at.elapsed().as_millis();
                task_reg.devices_found += 1;
                println!("found device at [{host_path}] in {elapsed_ms}ms");
                break 'task;
            }

            drop(task_reg); // lock is otherwise held for scope of loop
            thread::sleep(Duration::from_millis(100));
        }
    })
}

fn enumerate_ata_paths(dir_tree: fs::ReadDir) -> Vec<String> {
    let mut paths = Vec::with_capacity(24); // assumed that this controller has 24 ports

    for ent in dir_tree {
        let ent = ent.expect("i/o error reading PCI device directory tree");
        let info = ent.file_type().expect("i/o error reading PCI device file entry");

        if !info.is_dir() { continue; } // looking only for ata* directories

        let path_name = ent.file_name().to_string_lossy().into_owned();
        if !path_name.starts_with("ata") { continue; }

        paths.push(path_name); // found an ATA path
    }

    return paths;
}
