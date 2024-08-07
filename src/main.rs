use windows::{core::*,Win32::{Foundation::*,Storage::FileSystem::*,System::IO::*,System::Ioctl::*}};
use std::env::args;
use std::ffi::*;
use std::mem::*;

macro_rules! CTL_CODE
{
	($DeviceType:expr,$Function:expr,$Method:expr,$Access:expr) =>
	{
		($DeviceType<<16)|($Access<<14)|($Function<<2)|$Method
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

fn main()
{
	let argv:Vec<String>=args().collect();
	let cmd=argv.get(1);
	let addr=argv.get(2);
	let target_address:u64;
	let is_read:bool;
	let is_phys:bool;
	// Parse the command.
	match cmd
	{
		None =>
		{
			println!("Missing command argument!");
			return;
		}
		Some(c)=>
		{
			if c.eq(&String::from("readphys"))
			{
				is_phys=true;
				is_read=true;
			}
			else if c.eq(&String::from("read"))
			{
				is_phys=false;
				is_read=true;
			}
			else if c.eq(&String::from("writephys"))
			{
				println!("Physical Write is unimplemented!");
				return;
			}
			else if c.eq(&String::from("write"))
			{
				println!("Write is unimplemented!");
				return;
			}
			else
			{
				println!("Unknown command: {}!",c);
				return;
			}
		}
	}
	// Parse the address.
	match addr
	{
		None =>
		{
			println!("Missing target address!");
			return;
		}
		Some(a)=>
		{
			let x=u64::from_str_radix(a.as_str(), 16);
			match x
			{
				Ok(k)=>
				{
					target_address=k;
				}
				Err(e)=>
				{
					println!("Failed to parse address! {}",e);
					return;
				}
			}
		}
	}
	// Initialize driver interface.
	let device_handle:HANDLE;
	let mut b_array:[u8;4096]=[0;4096];
	unsafe
	{
		// For some weird reasons, "dwDesiredAccess" is u32 type, so we can't use GENERIC_READ.
		let hdev:Result<HANDLE>=CreateFileW(w!("\\\\.\\atadma"),0x80000000,FILE_SHARE_READ,None,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,None);
		match hdev
		{
			Ok(h) =>
			{
				device_handle=h;
			}
			Err(e) =>
			{
				println!("Failed to open atadma device! {}",e);
				return;
			}
		}
	}
	let req=DmaRequest
	{
		is_physical:is_phys,
		source:if is_read {target_address} else {b_array.as_mut_ptr() as u64},
		destination:if is_read {b_array.as_mut_ptr() as u64} else {target_address}
	};
	let r:Result<()>;
	unsafe
	{
		let inbuf:Option<*const c_void>=Some(&req as *const DmaRequest as *const c_void);
		let mut retlen:u32=0;
		r=DeviceIoControl(device_handle, IOCTL_DMA_READ, inbuf, size_of::<DmaRequest>() as u32, None, 0, Some(&mut retlen), None);
		let _=CloseHandle(device_handle);
	}
	match r
	{
		Ok(_)=>
		{
			if is_read
			{
				// Print out the result.
				for i in (0..4096).step_by(16)
				{
					print!("{:016x}\t",target_address+i);
					for j in 0..16
					{
						print!("{:02X} ",b_array[(i+j) as usize]);
					}
					print!("\t\t");
					for j in 0..16
					{
						let c:u8=b_array[(i+j) as usize];
						if c>=0x20 && c<=0x7f
						{
							print!("{}",c as char);
						}
						else
						{
							print!(".");
						}
					}
					println!("");
				}
			}
		}
		Err(e)=>
		{
			println!("DeviceIoControl failed! {}",e);
		}
	}
}
