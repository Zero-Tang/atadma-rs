import os
import pefile
import subprocess

# The purpose of this script is to skip WDF phase (FxDriverEntry).
# Or otherwise, the driver will fail to load due to WdfVersionBind.
if __name__=="__main__":
	# Call cargo to build the driver.
	r=subprocess.call(["cargo","make"])
	if r:
		print("Cargo returned error: {}".format(r))
	else:
		# Patch the entry point
		print("\n[Patch Entry] Patching driver entry point...")
		img=pefile.PE(os.path.join("target","debug","atadma_drv.sys"))
		# The real DriverEntry can be found in export table.
		entry_point=None
		for exp in img.DIRECTORY_ENTRY_EXPORT.symbols:
			if exp.name.decode('latin-1')=="DriverEntry":
				entry_point=exp.address
				break
		if entry_point is None:
			print("Failed to locate true entry point!")
		else:
			print("Located true entry point (DriverEntry) at RVA 0x{:X}!".format(entry_point))
			print("Found FxDriverEntry at 0x{:X}!".format(img.OPTIONAL_HEADER.AddressOfEntryPoint))
			# Then patch the driver.
			print("Replacing...")
			img.OPTIONAL_HEADER.AddressOfEntryPoint=entry_point
			img.write(os.path.join("target","debug","atadma_drv_fixed.sys"))
			img.close()
			# Sign the driver.
			subprocess.call(["signtool","sign","/v","/fd","SHA1","/f","ztnxtest.pfx",os.path.join("target","debug","atadma_drv_fixed.sys")])