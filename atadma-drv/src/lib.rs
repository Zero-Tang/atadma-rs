#![no_std]

#[cfg(not(test))] extern crate wdk_panic;

extern crate alloc;

use core::ffi::c_void;

use ntddk::*;
use wdk_alloc::WdkAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR:WdkAllocator = WdkAllocator;

mod disk;

use alloc::boxed::Box;

use wdk::println;
use wdk_sys::*;

use utf16_lit::utf16;

use disk::*;

pub const RUST_TAG: ULONG = u32::from_ne_bytes(*b"rust");

// See https://github.com/microsoft/windows-drivers-rs/issues/119
#[allow(non_snake_case)]
#[inline] pub const fn CTL_CODE(DeviceType:u32,Function:u32,Method:u32,Access:u32)->u32
{
	(DeviceType<<16)|(Access<<14)|(Function<<2)|Method
}

// See https://github.com/microsoft/windows-drivers-rs/issues/119
/// # Safety
/// This is emulating IoGetCurrentIrpStackLocation. Pointer might be NULL!
#[allow(non_snake_case)]
#[inline] pub unsafe fn IoGetCurrentIrpStackLocation(irp:*mut IRP)->PIO_STACK_LOCATION
{
	(*irp).Tail.Overlay.__bindgen_anon_2.__bindgen_anon_1.CurrentStackLocation
}

const IOCTL_DMA_READ:u32 = CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS);

const DEVICE_NAME:[u16;15]=utf16!("\\Device\\atadma\0");
const LINK_NAME:[u16;19]=utf16!("\\DosDevices\\atadma\0");

// Make UNICODE_STRING easier.
#[inline] pub fn constant_unicode_string(string:&[u16])->UNICODE_STRING
{
	UNICODE_STRING
	{
		Length:(string.len()*2) as u16,
		MaximumLength:(string.len()*2) as u16,
		Buffer:string.as_ptr() as *mut u16
	}
}

#[repr(C)]
struct DmaRequest
{
	destination:u64,
	source:u64,
	is_physical:bool
}

unsafe extern "C" fn driver_unload(driver:*mut DRIVER_OBJECT)
{
	let mut sym_name:UNICODE_STRING = constant_unicode_string(&LINK_NAME);
	let _ = IoDeleteSymbolicLink(&mut sym_name);
	// Release the Device-Extension!
	let dev_obj:PDEVICE_OBJECT = (*driver).DeviceObject;
	ExFreePool((*dev_obj).DeviceExtension);
	IoDeleteDevice(dev_obj);
}

unsafe extern "C" fn dispatch_create_close(_device:*mut DEVICE_OBJECT, irp:*mut IRP) -> NTSTATUS
{
	(*irp).IoStatus.Information = 0;
	// Don't understand why Status is not defined directly.
	(*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
	IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
	STATUS_SUCCESS
}

unsafe extern "C" fn dispatch_ioctl(device:*mut DEVICE_OBJECT, irp:*mut IRP) -> NTSTATUS
{
	let mut st:NTSTATUS = STATUS_INVALID_DEVICE_REQUEST;
	// The wdk-sys crate does not have IoGetCurrentIrpStackLocation macro.
	let irpsp:PIO_STACK_LOCATION = IoGetCurrentIrpStackLocation(irp);
	let ioctrl_code:u32 = (*irpsp).Parameters.DeviceIoControl.IoControlCode;
	// Dispatch the IOCTL.
	match ioctrl_code
	{
		IOCTL_DMA_READ =>
		{
			let req:*const DmaRequest = (*irp).AssociatedIrp.SystemBuffer as *const DmaRequest;
			st = STATUS_NO_SUCH_DEVICE;
			let disk_obj_p:*mut DiskObject = (*device).DeviceExtension as *mut DiskObject;
			if !(*disk_obj_p).device.is_null()
			{
				let disk_obj:&mut DiskObject = &mut (*disk_obj_p);
				st = STATUS_INSUFFICIENT_RESOURCES;
				let pa:PHYSICAL_ADDRESS = PHYSICAL_ADDRESS { QuadPart:(*req).source as i64 };
				let virt_ptr:PVOID = if (*req).is_physical
				{
					MmMapIoSpace(pa, PAGE_SIZE as u64, _MEMORY_CACHING_TYPE::MmCached)
				}
				else
				{
					(*req).source as PVOID
				};
				if !virt_ptr.is_null()
				{
					st = ata_copy_memory(disk_obj, (*req).destination as PVOID, virt_ptr);
					if (*req).is_physical
					{
						MmUnmapIoSpace(virt_ptr, PAGE_SIZE as u64);
					}
				}
				else if (*req).is_physical
				{
					println!("Failed to map physical address for 0x{:016X}!", (*req).source);
				}
			}
			println!("[atadma] Received DMA-Read request!");
		}
		x =>
		{
			println!("Unknown I/O Control Code: 0x{:08X}!", x);
		}
	}
	(*irp).IoStatus.Information = 0;
	// I don't understand why Status is not defined right inside IOSB.
	(*irp).IoStatus.__bindgen_anon_1.Status = st;
	IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
	st
}

/// # Safety
/// This function is the entry function called by Windows Kernel.
/// It includes lots of raw pointer operations, so it's unsafe.
#[export_name = "DriverEntry"]
pub unsafe extern "system" fn driver_entry(driver:&mut DRIVER_OBJECT, _registry_path:PCUNICODE_STRING) -> NTSTATUS
{
	let mut st:NTSTATUS;
	let mut dev_obj:PDEVICE_OBJECT = core::ptr::null_mut();
	let mut dev_name:UNICODE_STRING = constant_unicode_string(&DEVICE_NAME);
	let mut sym_name:UNICODE_STRING = constant_unicode_string(&LINK_NAME);
	// Setup dispatch routines.
	driver.MajorFunction[IRP_MJ_CREATE as usize] = Some(dispatch_create_close);
	driver.MajorFunction[IRP_MJ_CLOSE as usize] = Some(dispatch_create_close);
	driver.MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] = Some(dispatch_ioctl);
	driver.DriverUnload = Some(driver_unload);
	// Create device and symbolic link name.
	st = IoCreateDevice(driver, 0, &mut dev_name, FILE_DEVICE_UNKNOWN, FILE_DEVICE_SECURE_OPEN, 0, &mut dev_obj);
	if NT_SUCCESS(st)
	{
		st = IoCreateSymbolicLink(&mut sym_name, &mut dev_name);
		if NT_SUCCESS(st)
		{
			// WTF, SIZE_T is defined as u64 instead of usize???
			let disk_obj:Box<DiskObject>=Box::new(DiskObject::new());
			let disk_obj_ref=Box::leak(disk_obj);
			(*dev_obj).DeviceExtension = disk_obj_ref as *mut DiskObject as *mut c_void;
			st = find_disk(disk_obj_ref);
			if !NT_SUCCESS(st)
			{
				ExFreePool((*dev_obj).DeviceExtension);
				let _ = IoDeleteSymbolicLink(&mut sym_name);
				IoDeleteDevice(dev_obj);
			}
		}
		else
		{
			IoDeleteDevice(dev_obj);
		}
	}
	println!("Driver-Load Status: 0x{:X}", st);
	st
}
