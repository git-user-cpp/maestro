//! Each TTY or pseudo-TTY has to be associated with a device file in order to communicate with it.

use core::ffi::c_void;
use crate::device::DeviceHandle;
use crate::errno::Errno;
use crate::errno;
use crate::process::Process;
use crate::process::mem_space::MemSpace;
use crate::process::mem_space::ptr::SyscallPtr;
use crate::process::pid::Pid;
use crate::process::signal::Signal;
use crate::process::signal::SignalHandler;
use crate::syscall::ioctl;
use crate::tty::TTY;
use crate::tty::TTYHandle;
use crate::tty::WinSize;
use crate::tty::termios::Termios;
use crate::tty::termios;
use crate::util::io::IO;
use crate::util::io;
use crate::util::ptr::IntSharedPtr;

/// Structure representing a TTY device's handle.
pub struct TTYDeviceHandle {
	/// The device's TTY. If None, using the current process's TTY.
	tty: Option<TTYHandle>,
}

impl TTYDeviceHandle {
	/// Creates a new instance for the given TTY `tty`.
	/// If `tty` is None, the device works with the current process's TTY.
	pub fn new(tty: Option<TTYHandle>) -> Self {
		Self { tty }
	}

	/// Returns the current process and its associated TTY.
	fn get_tty(&self) -> Result<(IntSharedPtr<Process>, TTYHandle), Errno> {
		let proc_mutex = Process::get_current().unwrap();
		let proc_guard = proc_mutex.lock();
		let proc = proc_guard.get();

		let tty_mutex = self.tty.clone().unwrap_or_else(|| proc.get_tty());
		drop(proc_guard);

		Ok((proc_mutex, tty_mutex))
	}

	/// Checks whether the process is allowed to read from the TTY. If not, it is killed with a
	/// SIGTTIN signal.
	/// `process` is the process.
	/// `tty` is the TTY.
	/// This function must be called before performing the read operation.
	fn check_sigttin(&self, proc: &mut Process, tty: &TTY) -> Result<(), Errno> {
		if proc.get_pgid() != tty.get_pgrp() {
			if proc.is_signal_blocked(&Signal::SIGTTIN)
				|| proc.get_signal_handler(&Signal::SIGTTIN) == SignalHandler::Ignore
				|| proc.is_in_orphan_process_group() {
				return Err(errno!(EIO));
			}

			proc.kill_group(Signal::SIGTTIN, false);
		}

		Ok(())
	}

	/// Checks whether the process is allowed to write to the TTY. If not, it is killed with a
	/// SIGTTOU signal.
	/// `process` is the process.
	/// `tty` is the TTY.
	/// This function must be called before performing the write operation.
	fn check_sigttou(&self, proc: &mut Process, tty: &TTY) -> Result<(), Errno> {
		if tty.get_termios().c_lflag & termios::TOSTOP != 0 {
			if proc.is_signal_blocked(&Signal::SIGTTIN)
				|| proc.get_signal_handler(&Signal::SIGTTIN) == SignalHandler::Ignore {
				return Ok(());
			}
			if proc.is_in_orphan_process_group() {
				return Err(errno!(EIO));
			}

			proc.kill_group(Signal::SIGTTOU, false);
		}

		Ok(())
	}
}

impl DeviceHandle for TTYDeviceHandle {
	fn ioctl(
		&mut self,
		mem_space: IntSharedPtr<MemSpace>,
		request: u32,
		argp: *const c_void,
	) -> Result<u32, Errno> {
		let (proc_mutex, tty_mutex) = self.get_tty()?;
		let proc_guard = proc_mutex.lock();
		let proc = proc_guard.get_mut();
		let tty_guard = tty_mutex.lock();
		let tty = tty_guard.get_mut();

		match request {
			ioctl::TCGETS => {
				let mem_space_guard = mem_space.lock();
				let termios_ptr: SyscallPtr<Termios> = (argp as usize).into();
				let termios_ref = termios_ptr
					.get_mut(&mem_space_guard)?
					.ok_or_else(|| errno!(EFAULT))?;
				*termios_ref = tty.get_termios().clone();

				Ok(0)
			}

			// TODO Implement correct behaviours for each
			ioctl::TCSETS | ioctl::TCSETSW | ioctl::TCSETSF => {
				self.check_sigttou(proc, tty)?;

				let mem_space_guard = mem_space.lock();
				let termios_ptr: SyscallPtr<Termios> = (argp as usize).into();
				let termios = termios_ptr
					.get(&mem_space_guard)?
					.ok_or_else(|| errno!(EFAULT))?;
				tty.set_termios(termios.clone());

				Ok(0)
			}

			ioctl::TIOCGPGRP => {
				let mem_space_guard = mem_space.lock();
				let pgid_ptr: SyscallPtr<Pid> = (argp as usize).into();
				let pgid_ref = pgid_ptr
					.get_mut(&mem_space_guard)?
					.ok_or_else(|| errno!(EFAULT))?;
				*pgid_ref = tty.get_pgrp();

				Ok(0)
			}

			ioctl::TIOCSPGRP => {
				self.check_sigttou(proc, tty)?;

				let mem_space_guard = mem_space.lock();
				let pgid_ptr: SyscallPtr<Pid> = (argp as usize).into();
				let pgid = pgid_ptr
					.get(&mem_space_guard)?
					.ok_or_else(|| errno!(EFAULT))?;
				tty.set_pgrp(*pgid);

				Ok(0)
			}

			ioctl::TIOCGWINSZ => {
				let mem_space_guard = mem_space.lock();
				let winsize: SyscallPtr<WinSize> = (argp as usize).into();
				let winsize_ref = winsize
					.get_mut(&mem_space_guard)?
					.ok_or_else(|| errno!(EFAULT))?;
				*winsize_ref = tty.get_winsize().clone();

				Ok(0)
			}

			ioctl::TIOCSWINSZ => {
				let mem_space_guard = mem_space.lock();
				let winsize_ptr: SyscallPtr<WinSize> = (argp as usize).into();
				let winsize = winsize_ptr
					.get(&mem_space_guard)?
					.ok_or_else(|| errno!(EFAULT))?;

				// Dropping to avoid deadlock since `set_winsize` sends the SIGWINCH signal
				drop(proc_guard);
				tty.set_winsize(winsize.clone());

				Ok(0)
			}

			_ => Err(errno!(EINVAL)),
		}
	}
}

impl IO for TTYDeviceHandle {
	fn get_size(&self) -> u64 {
		if let Ok((_, tty_mutex)) = self.get_tty() {
			let tty_guard = tty_mutex.lock();
			let tty = tty_guard.get_mut();

			tty.get_available_size() as _
		} else {
			0
		}
	}

	fn read(&mut self, _offset: u64, buff: &mut [u8]) -> Result<(u64, bool), Errno> {
		let (proc_mutex, tty_mutex) = self.get_tty()?;
		let proc_guard = proc_mutex.lock();
		let proc = proc_guard.get_mut();
		let tty_guard = tty_mutex.lock();
		let tty = tty_guard.get_mut();

		self.check_sigttin(proc, tty)?;

		let (len, eof) = tty.read(buff);
		Ok((len as _, eof))
	}

	fn write(&mut self, _offset: u64, buff: &[u8]) -> Result<u64, Errno> {
		let (proc_mutex, tty_mutex) = self.get_tty()?;
		let proc_guard = proc_mutex.lock();
		let proc = proc_guard.get_mut();
		let tty_guard = tty_mutex.lock();
		let tty = tty_guard.get_mut();

		self.check_sigttou(proc, tty)?;

		tty.write(buff);
		Ok(buff.len() as _)
	}

	fn poll(&mut self, mask: u32) -> Result<u32, Errno> {
		let (_, tty_mutex) = self.get_tty()?;
		let tty_guard = tty_mutex.lock();
		let tty = tty_guard.get_mut();

		let mut result = 0;
		if mask & io::POLLIN != 0 && tty.get_available_size() > 0 {
			result |= io::POLLIN;
		}
		if mask & io::POLLOUT != 0 {
			result |= io::POLLOUT;
		}
		// TODO Implement every events

		Ok(result)
	}
}