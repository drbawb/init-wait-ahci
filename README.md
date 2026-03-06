# late-thunderbolt systemd service

I don't particularly expect this to be useful to anyone else, but if you need
it, godspeed and good luck...

## What's the problem?

If you have a not-so-reputable firmware vendor (i.e. run a cheap chinese mini
PC, and use a not-so-cheap and definitely-should-know-better JBOD) you may find
yourself in a situation like this:

- You have ZFS installed, maybe you even boot from ZFS
- You are using `zfs-import-cache.service` or something similar to import your
  pools at boot, and the `zfs-mount-generator` to make `.mount` units for the
  datasets on those pool(s).
- You have a Thunderbolt JBOD which has some or all of the disks for those
  pools. (Since the mini PC has very little in the way of internal I/O or
  expansion.)
- Nothing works!? `zpool-import` can't find disks! `dmesg` gets spammed with notifications
  that an AHCI controller is AWOL!?! Your system hangs!? WTF?

Here is what I figured out:

1. My firmware has virtually zero options to configure thunderbolt security
2. The security is stuck at "user" level requiring `udev` or `boltd` to
   authorize the device after system start-up.
3. Trying to be "helpful" my firmware sets up a PCIe tunnel and spins up the
   JBOD for me during early boot.

This *seems* like it would be a nice thing. Your early initramfs sees an AHCI
controller, the drives are ready, you could theoretically maybe even select them
as boot volumes? Cool, right?

NOT COOL.

As soon as ANYTHING on your system tries to load the `thunderbolt` module the
PCIe tunnel gets rebuilt and the JBOD briefly resets. You now have a bunch of
orphaned block devices bound to an AHCI driver that is completely out of sync
with the reality of the device it's talking to.

Until the AHCI driver realizes the 4 ports (and 20 fake ports) have timed out
and reinitailizes itself your system will be horrendously slow and unstable if
anything tries to touch those block devices. (Like, idk, a unit in your system
bringup like `zpool-import-cache.service`.)

## ⚠️IMPORTANT DISCLAIMER ⚠️

If you are actually going to try and use this **THERE ARE HARDCODED PCI BUS
IDS, DEVICE IDS, AND VENDOR IDS.**

You *absolutely* must change at least `src/main.rs` and `dist/hooks/rm-tb-pci`
this *WILL NOT WORK* on your system as-is unless you happened to buy the same
exact hardware as me, which I somehow doubt.

## How did you fix it?

I expended idk something like 12 hours of my life to bring you ...

- This will build a local package for an Arch Linux system using systemd for
  both the init and initrd; i.e. you MUST have `systemd` in your initcpio
  `HOOKS=(...)` array.

- It has both an initrd component plus a userspace component. It is primarily
  implemented as two systemd units plus some dependencies for them.

Basically what I did is this:

0. I've blacklisted the `thunderbolt` module so it isn't loaded automatically.
   (This is not part of this package at the time I'm writing this readme.)

1. A unit is added to `initrd` which WHACKS the whole PCIe topology
   representing evertyhing on the downstream side of the TBT3 bridge
   for my JBOD. This forces my particular JBOD (OWC Thunderbay 4) to
   reset and spin-down the drives, which takes a while to return.

2. Boot carries on "as normal" until it tries to mount local filesystems

3. I pull in `late-thunderbolt.service` which modprobes the thunderbolt
   driver, pulls in `bolt.service`, and waits for the drives to spin-up
   with a binary I wrote. (The rust program.)

4. I have a separate `zfs-import-data.service` unit (also not packaged) so that
   the pools behind the JBOD can be imported separately (/ fail separately) from
   importing my root zpool. If you don't have zfs-on-root this is maybe less
   important to you.

The rust program `init-wait-ahci` is pretty straightforward:

1. It waits for the AHCI controller to show up (or not...)

2. It discovers the downstream `/ata*` device nodes

3. Some worker threads wait for those to have valid SCSI targets, at which
   point I just kind of assume they are bound to block device nodes.

4. As soon as we find four SCSI targets (the max the enclosure supports) we
   bail out. - This should in theory allow this to work if you swapped this out
   with a JBOD that supports more disks like their 8-bay model.

### Do you recommend anybody use this?

Fuck. No.

I thought I was smarter than everyone who said "don't use USB/TB for ZFS", I
was fucking wrong, they were right.

*Get a real HBA, connect your drives with SAS/SATA, be done with it.*

This adds about 1.5 minutes to my boot time. (The drives basically have to spin
up, down, then up again; and OF COURSE the enclosure has sequential spin up :D)
It is abject suffering. I'm seriously considering buying a PDU and like a
raspberry pi or something just so I can properly power sequence my equipment
the way my ancestors did. My shelf has become a victim of "donglegate". It
sucks.

I've *also* entertained grabbing UefiTool and patching my BIOS so it just
doesn't waste time bringing up the disks _I can't fucking use._

I'm not sure who thought this was a good idea, or really who to even blame?
Should the mini PC not be bringing up the tunnel in early boot? Does the JBOD
have buggy firwmare and it should more gracefully handle the thunderbolt module
loading in userspace? Is the thunderbolt kmod a piece of crap? WHO KNOWS.

This fixed the issue, for me, at the cost of my life and my sanity. I can offer
you no solace and no warranty. In fact quite the opposite: I expect forcibly
removing the PCIe devices is probably not great for the enclosure or the
drives. Thankfully this is a "server" that will boot "rarely" ...

Please. I'm begging you. Get a real computer with a PCIe slot, or a built-in
storage controller. Do not attach storage you need during system bringup via
TBT3.

### Appendix A: PCIe Toplogy

This is the PCIe topology I'm working with, just so you can somewhat visualize
what the hell this script does:

```
$ lspci -tv

...snip...

+-04.0  Advanced Micro Devices, Inc. [AMD] Phoenix Dummy Host Bridge
+-04.1-[65-c4]----00.0-[66-c4]--+-01.0-[67]----00.0  ASMedia Technology Inc. ASM1164 Serial ATA AHCI Controller
|                               +-02.0-[68]----00.0  Intel Corporation JHL7440 Thunderbolt 3 USB Controller [Titan Ridge DD 2018]
|                               \-04.0-[69-c4]--

...snip...

$ lspci

00:04.1 PCI bridge: Advanced Micro Devices, Inc. [AMD] Family 19h USB4/Thunderbolt PCIe tunnel
...snip...
65:00.0 PCI bridge: Intel Corporation JHL7440 Thunderbolt 3 Bridge [Titan Ridge DD 2018] (rev 06)
66:01.0 PCI bridge: Intel Corporation JHL7440 Thunderbolt 3 Bridge [Titan Ridge DD 2018] (rev 06)
66:02.0 PCI bridge: Intel Corporation JHL7440 Thunderbolt 3 Bridge [Titan Ridge DD 2018] (rev 06)
66:04.0 PCI bridge: Intel Corporation JHL7440 Thunderbolt 3 Bridge [Titan Ridge DD 2018] (rev 06)
67:00.0 SATA controller: ASMedia Technology Inc. ASM1164 Serial ATA AHCI Controller (rev 02)
68:00.0 USB controller: Intel Corporation JHL7440 Thunderbolt 3 USB Controller [Titan Ridge DD 2018] (rev 06)
..snip..
```

Not 100% positive, since I haven't used it, and probably can't reliably use it
with this absolute hackjob of an initrd, but I believe 69:00 is the daisy
chained Thunderbolt 3 port on the back of the disk enclosure.

I am wacking `65:00.0` just because it's kind of one step removed from whatever
you call my host interface (NHI) so that makes it easier to revive later. If I
wack `00:04.1` it seems like the only way to get stuff back is with judicious
pokes of `/sys/bus/pci/.../rescan` nodes, but if I wack the Intel TBT3 bridge
chip instead it seems to just kind of come back by itself after a `modprobe -i
thunderbolt` and everything "just works" after that.

Your mileage and PCIe topology may vary.

This *also* seems to be the "gentlest" on the enclosure; it seems like removing
the bridge makes it think the host is gone or maybe has gone to sleep, and so
it does what appears to be a more graceful reset. (It individually spins down
each disk.)

If I remove the AMD dummy host bridge it just kinda dies... and I would
probably want some random sleeps etc. to make sure it's fully shutdown. Screw
that.
