#![no_std]

#[cfg(not(test))]
extern crate wdk_panic;

use ntddk::*;
#[cfg(not(test))]
use wdk_alloc::WDKAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WDKAllocator=WDKAllocator;

mod disk;

use wdk_sys::*;
use wdk::println;

use disk::*;

// See https://github.com/microsoft/windows-drivers-rs/issues/119
#[macro_export]
macro_rules! CTL_CODE
{
	($DeviceType:expr,$Function:expr,$Method:expr,$Access:expr) =>
	{
		($DeviceType<<16)|($Access<<14)|($Function<<2)|$Method
	};
}

// See https://github.com/microsoft/windows-drivers-rs/issues/119
macro_rules! IoGetCurrentIrpStackLocation
{
	($irp:expr) =>
	{
		(*$irp).Tail.Overlay.__bindgen_anon_2.__bindgen_anon_1.CurrentStackLocation
	};
}

const IOCTL_DMA_READ:u32=CTL_CODE!(FILE_DEVICE_UNKNOWN,0x801,METHOD_BUFFERED,FILE_ANY_ACCESS);

#[repr(C)]
struct DmaRequest
{
	destination:u64,
	source:u64,
	is_physical:bool,
}

unsafe extern "C" fn driver_unload(driver:*mut DRIVER_OBJECT)->()
{
	// let sym_name_raw="\\DosDevices\\atadma"
	let mut sym_name:UNICODE_STRING=UNICODE_STRING{Length:0,MaximumLength:0,Buffer:0 as *mut u16};
	let sym_name_raw:[u16;19]=[0x5c,0x44,0x6f,0x73,0x44,0x65,0x76,0x69,0x63,0x65,0x73,0x5c,0x61,0x74,0x61,0x64,0x6d,0x61,0];
	RtlInitUnicodeString(&mut sym_name,sym_name_raw.as_ptr());
	let _=IoDeleteSymbolicLink(&mut sym_name);
	// Release the Device-Extension!
	let dev_obj:PDEVICE_OBJECT=(*driver).DeviceObject;
	ExFreePool((*dev_obj).DeviceExtension);
	IoDeleteDevice(dev_obj);
}

unsafe extern "C" fn dispatch_create_close(_device:*mut DEVICE_OBJECT,irp:*mut IRP)->NTSTATUS
{
	(*irp).IoStatus.Information=0;
	// Don't understand why Status is not defined directly.
	(*irp).IoStatus.__bindgen_anon_1.Status=STATUS_SUCCESS;
	IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
	return STATUS_SUCCESS;
}

unsafe extern "C" fn dispatch_ioctl(device:*mut DEVICE_OBJECT,irp:*mut IRP)->NTSTATUS
{
	let mut st:NTSTATUS=STATUS_INVALID_DEVICE_REQUEST;
	// The wdk-sys crate does not have IoGetCurrentIrpStackLocation macro.
	let irpsp:PIO_STACK_LOCATION=IoGetCurrentIrpStackLocation!(irp);
	let ioctrl_code:u32=(*irpsp).Parameters.DeviceIoControl.IoControlCode;
	// Dispatch the IOCTL.
	match ioctrl_code
	{
		IOCTL_DMA_READ =>
		{
			let req:*const DmaRequest=(*irp).AssociatedIrp.SystemBuffer as *const DmaRequest;
			st=STATUS_NO_SUCH_DEVICE;
			let disk_obj_p:*mut DiskObject=(*device).DeviceExtension as *mut DiskObject;
			if (*disk_obj_p).device!=core::ptr::null_mut() as PDEVICE_OBJECT
			{
				let disk_obj:&mut DiskObject=&mut (*disk_obj_p);
				st=STATUS_INSUFFICIENT_RESOURCES;
				let pa:PHYSICAL_ADDRESS=PHYSICAL_ADDRESS{QuadPart:(*req).source as i64};
				let virt_ptr:PVOID=if (*req).is_physical {MmMapIoSpace(pa,PAGE_SIZE as u64,_MEMORY_CACHING_TYPE::MmCached)} else {(*req).source as PVOID};
				if virt_ptr!=core::ptr::null_mut()
				{
					st=ata_copy_memory(disk_obj,(*req).destination as PVOID,virt_ptr);
					if (*req).is_physical
					{
						MmUnmapIoSpace(virt_ptr, PAGE_SIZE as u64);
					}
				}
				else
				{
					if (*req).is_physical
					{
						println!("Failed to map physical address for 0x{:016X}!",(*req).source);
					}
					else
					{
						println!("You specified a null pointer!");
					}
				}
			}
			println!("[atadma] Received DMA-Read request!");
		}
		x =>
		{
			println!("Unknown I/O Control Code: 0x{:08X}!",x);
		}
	}
	(*irp).IoStatus.Information=0;
	// I don't understand why Status is not defined right inside IOSB.
	(*irp).IoStatus.__bindgen_anon_1.Status=st;
	IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
	return st;
}

#[export_name="DriverEntry"]
pub unsafe extern "system" fn driver_entry(driver:&mut DRIVER_OBJECT,_registry_path: PCUNICODE_STRING)->NTSTATUS
{
	let mut st:NTSTATUS;
	let mut dev_name:UNICODE_STRING=UNICODE_STRING{Length:0,MaximumLength:0,Buffer:0 as *mut u16};
	let mut sym_name:UNICODE_STRING=UNICODE_STRING{Length:0,MaximumLength:0,Buffer:0 as *mut u16};
	let mut dev_obj:PDEVICE_OBJECT=core::ptr::null_mut();
	// Use "utf16str2array.py" script to generate the raw array.
	// let dev_name_raw="\\Device\\atadma"
	let dev_name_raw:[u16;15]=[0x5c,0x44,0x65,0x76,0x69,0x63,0x65,0x5c,0x61,0x74,0x61,0x64,0x6d,0x61,0];
	// let sym_name_raw="\\DosDevices\\atadma"
	let sym_name_raw:[u16;19]=[0x5c,0x44,0x6f,0x73,0x44,0x65,0x76,0x69,0x63,0x65,0x73,0x5c,0x61,0x74,0x61,0x64,0x6d,0x61,0];
	RtlInitUnicodeString(&mut dev_name,dev_name_raw.as_ptr());
	RtlInitUnicodeString(&mut sym_name,sym_name_raw.as_ptr());
	// Setup dispatch routines.
	(*driver).MajorFunction[IRP_MJ_CREATE as usize]=Some(dispatch_create_close);
	(*driver).MajorFunction[IRP_MJ_CLOSE as usize]=Some(dispatch_create_close);
	(*driver).MajorFunction[IRP_MJ_DEVICE_CONTROL as usize]=Some(dispatch_ioctl);
	(*driver).DriverUnload=Some(driver_unload);
	// Create device and symbolic link name.
	st=IoCreateDevice(driver,0,&mut dev_name,FILE_DEVICE_UNKNOWN,FILE_DEVICE_SECURE_OPEN,0,&mut dev_obj);
	if NT_SUCCESS(st)
	{
		st=IoCreateSymbolicLink(&mut sym_name,&mut dev_name);
		if NT_SUCCESS(st)
		{
			// WTF, SIZE_T is defined as u64 instead of usize???
			(*dev_obj).DeviceExtension=ExAllocatePool(_POOL_TYPE::NonPagedPool,size_of::<DiskObject>() as u64);
			if (*dev_obj).DeviceExtension==core::ptr::null_mut()
			{
				let _=IoDeleteSymbolicLink(&mut sym_name);
				IoDeleteDevice(dev_obj);
				st=STATUS_INSUFFICIENT_RESOURCES;
			}
			else
			{
				let disk_obj:*mut DiskObject=(*dev_obj).DeviceExtension as *mut DiskObject;
				st=find_disk(&mut (*disk_obj));
				if !NT_SUCCESS(st)
				{
					ExFreePool((*dev_obj).DeviceExtension);
					let _=IoDeleteSymbolicLink(&mut sym_name);
					IoDeleteDevice(dev_obj);
				}
			}
		}
		else
		{
			IoDeleteDevice(dev_obj);
		}
	}
	println!("Driver-Load Status: 0x{:X}",st);
	return st;
}