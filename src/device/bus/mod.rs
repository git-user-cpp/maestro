/// This module implements internal buses, including PCI and USB.

pub mod pci;

use crate::errno::Errno;
use crate::util::boxed::Box;
use crate::util::container::vec::Vec;
use crate::util::lock::mutex::Mutex;
use crate::util::lock::mutex::MutexGuard;

/// The list of buses connected to the CPU.
static mut BUSES: Mutex<Vec<Box<dyn Bus>>> = Mutex::new(Vec::new());

/// Trait representing a bus.
pub trait Bus {
	/// Returns the name of the bus.
	fn get_name(&self) -> &str;

	/// Tells whether the bus is a hotplug bus.
	fn is_hotplug(&self) -> bool;

	// TODO
}

// TODO Function to get the list of buses
// TODO Fucntion to get a bus with given name

/// Detects internal buses and registers them.
pub fn detect() -> Result<(), Errno> {
	let mut pci_manager = pci::PCIManager {};
	let devices = pci_manager.scan();

	// TODO Move into PCI scan itself?
	for i in 0..devices.len() {
		let dev = &devices[i];
		// TODO rm
		crate::println!("-> {:x} {:x} {:x} {:x}", dev.get_device_id(), dev.get_vendor_id(),
			dev.get_class(), dev.get_subclass());
		// TODO Allocate DMA zones
	}

	let mutex = unsafe {
		&mut BUSES
	};
	let mut guard = MutexGuard::new(mutex);
	let buses = guard.get_mut();
	buses.push(Box::new(pci_manager)?)?;

	Ok(())
}
