# atadma-rs
ATA-based DMA-attacking PoC tool written in Rust

## Introduction
This PoC is modified from [ddma](https://github.com/btbd/ddma) by [btbd](https://github.com/btbd). Both the CLI-tool and the Windows driver are written in Rust.

## Supported Platforms
OS: Any Windows with x64 support. This project does not use new kernel APIs so it should be able to run on any x64 Windows. \
The system must have at least one SATA disk.

## Build
**IMPORTANT**: Since 0.3.0 version of [WDK for Rust](https://github.com/microsoft/windows-drivers-rs), you are no longer required to use nightly toolchain of Rust.

To build the caller program, just use the standard way to build a Rust program:
```
cargo build
```

To build the driver program, you need the following pre-requisites:

- Mount [EWDK11 with VS Build Tools 17.8.6](https://docs.microsoft.com/en-us/legal/windows/hardware/enterprise-wdk-license-2022) to V: drive.
- Install [LLVM](https://github.com/llvm/llvm-project/releases). This is required to generate WDK-related bindings.

Then in `atadma-drv` directory, start building.
```
cargo make
```
If this is your first time building the driver, make sure that your console is under Administrator privilege, and that you have mounted EWDK11. The `cargo` will have to build the WDK crates for you.
···
V:\LaunchBuildEnv.cmd
cargo make
···

## Run
Install the driver in Administrator privilege:
```
sc create atadma type= kernel binPath= <Path to driver file> DisplayName=atadma
sc start atadma
```
Note that this command does not install the driver permanantly. You need to restart after system reboot. `sc start atadma` is good enough.

**Warning**: This program will write to the first 8 sectors of a disk. Hence, if the system crashes while this PoC is in DMA operation, your disk head will be destroyed. **ONLY YOU WILL BE RESPONSIBLE FOR POTENTIAL DATA LOSSES!** \
In other words, **YOU MUST AT LEAST BACKUP THE FIRST EIGHT SECTORS OF YOUR DISK!**. \
For virtual machines, you may simply use snapshots.

Then execute the program. It does not require Administrator privilege and can be placed anywhere.
```
atadma-rs <command> <address>
```

To unload the driver:
```
sc stop atadma
sc delete atadma
```

The `println!` macro provided by WDK crate will actually call `DbgPrint`. Therefore, to see debug outputs on debugger, execute the following command in WinDbg:
```
ed nt!Kd_DEFAULT_Mask f
```
If you need to make this setting permanent, you will need to modify debugee's registry:
```
reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Debug Print Filter" /v "DEFAULT" /t REG_DWORD /d 15 /f
```

### Command
There are four commands:

- `read` command can be used to read any kernel virtual address.
- `readphys` command can be used to read any physical address.
- `write` and `writephys` are reserved unimplemented commands.

### Address
The address must be specified in hexadecimal, case-insensitive, and without `0x` prefix.

## Theory
This PoC exploits the DMA capability from AHCI controllers by purposefully specifying DMA Flag in [ATA_PASS_THROUGH_DIRECT structure](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddscsi/ns-ntddscsi-_ata_pass_through_direct) to transfer data between disk and data. \
Simply put, this PoC will write content into the disk then read from the disk in order to perform the DMA attack.

Writing to protected memory means reading from disk and specify the destination to be the protected memory. \
Reading from protected memory means writing to disk and specify the source to be the protected memory.

## License
This repository is licensed under the [MIT License](./LICENSE).