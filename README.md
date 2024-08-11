# atadma-rs
ATA-based DMA-attacking PoC tool written in Rust

## Introduction
This PoC is modified from [ddma](https://github.com/btbd/ddma). Both the CLI-tool and Windows driver are written in Rust.

## Supported Platforms
OS: Any Windows with x64 support. This project does not use new kernel APIs so it should be able to run on any x64 Windows. \
The system must have at least one SATA disk.

## Build
Use standard method to compile this Rust program:
```
cargo build
```

## Run
Install the driver in Administrator privilege:
```
sc create atadma type= kernel binPath= <Path to atadma.sys> DisplayName=atadma
sc start atadma
```
Note that this command does not install the driver permanantly. You need to re-install after system reboot.

Then execute the program. It does not require Administrator privilege and can be placed anywhere.
```
atadma-rs <command> <address>
```

To unload the driver:
```
sc stop atadma
sc delete atadma
```

### Command
There are four commands:

- `read` command can be used to read any kernel virtual address.
- `readphys` command can be used to read any physical address.
- `write` and `writephys` are reserved unimplemented commands.

### Address
The address must be specified in hexadecimal, case-insensitive, without `0x` prefix.

## Theory
This PoC exploits the DMA capability from AHCI controllers by purposefully specifying DMA Flag in [ATA_PASS_THROUGH_DIRECT structure](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddscsi/ns-ntddscsi-_ata_pass_through_direct) to transfer data between disk and data. \
Simply put, this PoC will write content into the disk then read from the disk in order to perform the DMA attack.

Writing to protected memory means reading from disk and specify the destination to be the protected memory. \
Reading from protected memory means writing to disk and specify the source to be the protected memory.

## License
This repository is licensed under the [MIT License](./LICENSE).