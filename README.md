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

# IMPORTANT DISCLAIMER

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

## Do you recommend anybody use this?

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
