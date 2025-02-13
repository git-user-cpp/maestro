/*
 * Copyright 2024 Luc Lenôtre
 *
 * This file is part of Maestro.
 *
 * Maestro is free software: you can redistribute it and/or modify it under the
 * terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or (at your option) any later
 * version.
 *
 * Maestro is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
 * A PARTICULAR PURPOSE. See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * Maestro. If not, see <https://www.gnu.org/licenses/>.
 */

//! The `symlinkat` syscall allows to create a symbolic link.

use super::util::at;
use crate::{
	file::{
		fd::FileDescriptorTable,
		vfs,
		vfs::{ResolutionSettings, Resolved},
		FileType, Stat,
	},
	process::{mem_space::copy::SyscallString, Process},
	sync::mutex::Mutex,
	syscall::Args,
	time::{
		clock::{current_time, CLOCK_REALTIME},
		unit::TimestampScale,
	},
};
use core::ffi::c_int;
use utils::{
	collections::path::PathBuf,
	errno,
	errno::{EResult, Errno},
	limits::SYMLINK_MAX,
	ptr::arc::Arc,
};

pub fn symlinkat(
	Args((target, newdirfd, linkpath)): Args<(SyscallString, c_int, SyscallString)>,
	rs: ResolutionSettings,
	fds: Arc<Mutex<FileDescriptorTable>>,
) -> EResult<usize> {
	let target_slice = target.copy_from_user()?.ok_or_else(|| errno!(EFAULT))?;
	if target_slice.len() > SYMLINK_MAX {
		return Err(errno!(ENAMETOOLONG));
	}
	let target = PathBuf::try_from(target_slice)?;
	let linkpath = linkpath
		.copy_from_user()?
		.map(PathBuf::try_from)
		.transpose()?;
	// Create link
	let resolved = at::get_file(&fds.lock(), rs.clone(), newdirfd, linkpath.as_deref(), 0)?;
	match resolved {
		Resolved::Creatable {
			parent,
			name,
		} => {
			let ts = current_time(CLOCK_REALTIME, TimestampScale::Second)?;
			let file = vfs::create_file(
				parent,
				name,
				&rs.access_profile,
				Stat {
					mode: FileType::Link.to_mode() | 0o777,
					ctime: ts,
					mtime: ts,
					atime: ts,
					..Default::default()
				},
			)?;
			// TODO remove file on failure
			file.node()
				.ops
				.write_content(&file.node().location, 0, target.as_bytes())?;
		}
		Resolved::Found(_) => return Err(errno!(EEXIST)),
	}

	Ok(0)
}
