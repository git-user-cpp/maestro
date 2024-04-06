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

//! KernFS node utilities.

use crate::{
	file::{
		fs::{Filesystem, NodeOps},
		perm,
		perm::{Gid, Uid},
		DirEntry, FileType, INode, Mode,
	},
	time::unit::Timestamp,
};
use core::{any::Any, fmt::Debug, iter};
use utils::{errno, errno::EResult, ptr::cow::Cow};

/// Trait representing a node in a kernfs.
pub trait KernFSNode: Any + Debug + NodeOps {
	/// Returns the number of hard links to the node.
	fn get_hard_links_count(&self) -> u16 {
		1
	}

	/// Sets the number of hard links to the node.
	fn set_hard_links_count(&mut self, _hard_links_count: u16) {}

	/// Returns the type of the file.
	fn get_file_type(&self) -> FileType;

	/// Returns the permissions of the file.
	fn get_mode(&self) -> Mode {
		0o444
	}

	/// Sets the permissions of the file.
	fn set_mode(&mut self, _mode: Mode) {}

	/// Returns the UID of the file's owner.
	fn get_uid(&self) -> Uid {
		perm::ROOT_UID
	}

	/// Sets the UID of the file's owner.
	fn set_uid(&mut self, _uid: Uid) {}

	/// Returns the GID of the file's owner.
	fn get_gid(&self) -> Gid {
		perm::ROOT_GID
	}

	/// Sets the GID of the file's owner.
	fn set_gid(&mut self, _gid: Gid) {}

	/// Returns the timestamp of the last access to the file.
	fn get_atime(&self) -> Timestamp {
		0
	}

	/// Sets the timestamp of the last access to the file.
	fn set_atime(&mut self, _ts: Timestamp) {}

	/// Returns the timestamp of the last modification of the file's metadata.
	fn get_ctime(&self) -> Timestamp {
		0
	}

	/// Sets the timestamp of the last modification of the file's metadata.
	fn set_ctime(&mut self, _ts: Timestamp) {}

	/// Returns the timestamp of the last modification of the file's content.
	fn get_mtime(&self) -> Timestamp {
		0
	}

	/// Sets the timestamp of the last modification of the file's content.
	fn set_mtime(&mut self, _ts: Timestamp) {}

	/// If the current node is a directory, tells whether it is empty.
	///
	/// If the node is not a directory, the function return `true`.
	fn is_directory_empty(&self) -> EResult<bool> {
		let mut prev = 0;
		while let Some((entry, off)) = self.next_entry(prev)? {
			if entry.name.as_ref() != b"." && entry.name.as_ref() != b".." {
				return Ok(false);
			}
			prev = off;
		}
		Ok(true)
	}

	/// Adds the `entry` to the directory.
	///
	/// It is the caller's responsibility to ensure there is no two entry with the same name.
	///
	/// If the node is not a directory, the function does nothing.
	fn add_entry(&mut self, _entry: DirEntry<'_>) -> EResult<()> {
		Err(errno!(EPERM))
	}
	/// Removes the directory entry at the given offset `off`.
	///
	/// If the node is not a directory, the function does nothing.
	fn remove_entry(&mut self, _off: u64) {}
}

/// A kernfs node with the default behaviour for each file type.
#[derive(Debug)]
pub struct DefaultNode {
	/// The number of hard links to the node.
	hard_links_count: u16,

	/// The directory's owner user ID.
	uid: Uid,
	/// The directory's owner group ID.
	gid: Gid,
	/// The type of the file.
	file_type: FileType,
	/// The directory's permissions.
	perms: Mode,

	/// Timestamp of the last modification of the metadata.
	ctime: Timestamp,
	/// Timestamp of the last modification of the file.
	mtime: Timestamp,
	/// Timestamp of the last access to the file.
	atime: Timestamp,
}

impl DefaultNode {
	/// Creates a new node.
	///
	/// Arguments:
	/// - `uid` is the node owner's user ID
	/// - `gid` is the node owner's group ID
	/// - `file_type` is the type of the node
	/// - `perms` is the node's permissions
	///
	/// Timestamps are zeroed by default.
	pub fn new(uid: Uid, gid: Gid, file_type: FileType, perms: Mode) -> Self {
		Self {
			hard_links_count: 1,

			uid,
			gid,
			file_type,
			perms,

			ctime: 0,
			mtime: 0,
			atime: 0,
		}
	}
}

impl KernFSNode for DefaultNode {
	fn get_hard_links_count(&self) -> u16 {
		self.hard_links_count
	}

	fn set_hard_links_count(&mut self, hard_links_count: u16) {
		self.hard_links_count = hard_links_count;
	}

	fn get_file_type(&self) -> FileType {
		self.file_type
	}

	fn get_mode(&self) -> Mode {
		self.perms
	}

	fn set_mode(&mut self, mode: Mode) {
		self.perms = mode;
	}

	fn get_uid(&self) -> Uid {
		self.uid
	}

	fn set_uid(&mut self, uid: Uid) {
		self.uid = uid;
	}

	fn get_gid(&self) -> Gid {
		self.gid
	}

	fn set_gid(&mut self, gid: Gid) {
		self.gid = gid;
	}

	fn get_atime(&self) -> Timestamp {
		self.atime
	}

	fn set_atime(&mut self, ts: Timestamp) {
		self.atime = ts;
	}

	fn get_ctime(&self) -> Timestamp {
		self.ctime
	}

	fn set_ctime(&mut self, ts: Timestamp) {
		self.ctime = ts;
	}

	fn get_mtime(&self) -> Timestamp {
		self.mtime
	}

	fn set_mtime(&mut self, ts: Timestamp) {
		self.mtime = ts;
	}
}

impl NodeOps for DefaultNode {
	fn read_content(
		&self,
		inode: INode,
		fs: &dyn Filesystem,
		off: u64,
		buf: &mut [u8],
	) -> EResult<u64> {
		todo!()
	}

	fn write_content(
		&self,
		inode: INode,
		fs: &dyn Filesystem,
		off: u64,
		buf: &[u8],
	) -> EResult<u64> {
		todo!()
	}

	fn entry_by_name<'n>(
		&self,
		inode: INode,
		fs: &dyn Filesystem,
		name: &'n [u8],
	) -> EResult<Option<DirEntry<'n>>> {
		todo!()
	}

	fn next_entry(
		&self,
		inode: INode,
		fs: &dyn Filesystem,
		off: u64,
	) -> EResult<Option<(DirEntry<'static>, u64)>> {
		todo!()
	}
}

// TODO refactor to make it a format-like macro
/// Implementation of the [`IO::read`] function for a node that is built dynamically from a
/// sequence of strings taken from `iter`.
///
/// `off` and `buff` are the arguments from [`IO::read`].
///
/// If the iterator returns an error element, the function stops and returns the error.
pub fn content_chunks<'s, I: Iterator<Item = EResult<&'s [u8]>>>(
	off: u64,
	buff: &mut [u8],
	iter: I,
) -> EResult<u64> {
	let mut node_cursor = 0;
	let mut buff_cursor = 0;
	for chunk in iter {
		let chunk = chunk?;
		// If at least a part of the chunk is in range, copy
		if node_cursor + chunk.len() as u64 >= off {
			// Begin and size of the range in the chunk to copy
			let begin = off.saturating_sub(node_cursor) as usize;
			let size = chunk.len().saturating_sub(begin);
			buff[buff_cursor..(buff_cursor + size)].copy_from_slice(&chunk[begin..(begin + size)]);
			buff_cursor += size;
			// If the end of the output buffer is reached, break
			if buff_cursor >= buff.len() {
				break;
			}
		}
		node_cursor += chunk.len() as u64;
	}
	Ok(node_cursor)
}

/// A static symbolic link pointing to a constant target.
#[derive(Debug)]
pub struct StaticLink<const TARGET: &'static [u8]>;

impl<const TARGET: &'static [u8]> KernFSNode for StaticLink<TARGET> {
	fn get_file_type(&self) -> FileType {
		FileType::Link
	}

	fn get_mode(&self) -> Mode {
		0o777
	}
}

impl<const TARGET: &'static [u8]> NodeOps for StaticLink<TARGET> {
	fn read_content(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		off: u64,
		buf: &mut [u8],
	) -> EResult<u64> {
		content_chunks(off, buf, iter::once(TARGET))
	}

	fn write_content(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		_off: u64,
		_buf: &[u8],
	) -> EResult<u64> {
		Err(errno!(EACCES))
	}

	fn entry_by_name<'n>(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		_name: &'n [u8],
	) -> EResult<Option<DirEntry<'n>>> {
		Err(errno!(ENOTDIR))
	}

	fn next_entry(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		_off: u64,
	) -> EResult<Option<(DirEntry<'static>, u64)>> {
		Err(errno!(ENOTDIR))
	}
}

/// A read-only virtual directory used to point to other nodes.
pub trait StaticDirNode: Debug {
	/// Name/node pairs representing the entries of the directory, alphabetically sorted by name.
	///
	/// **Warning**: If this array is not sorted correctly, the behaviour of
	/// [`NodeOps::entry_by_name`] is undefined.
	const ENTRIES: &'static [(&'static [u8], &'static dyn KernFSNode)];
}

impl<N: StaticDirNode + 'static> KernFSNode for N {
	fn get_file_type(&self) -> FileType {
		FileType::Directory
	}

	fn get_mode(&self) -> Mode {
		0o555
	}

	fn get_uid(&self) -> Uid {
		0
	}

	fn get_gid(&self) -> Gid {
		0
	}
}

impl<N: StaticDirNode> NodeOps for N {
	fn read_content(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		_off: u64,
		_buf: &mut [u8],
	) -> EResult<u64> {
		Err(errno!(EISDIR))
	}

	fn write_content(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		_off: u64,
		_buf: &[u8],
	) -> EResult<u64> {
		Err(errno!(EISDIR))
	}

	fn entry_by_name<'n>(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		name: &'n [u8],
	) -> EResult<Option<DirEntry<'n>>> {
		let Some(index) = Self::ENTRIES.binary_search_by(|(n, _)| (*n).cmp(name)) else {
			return Ok(None);
		};
		let (name, node) = Self::ENTRIES[index];
		Ok(Some(DirEntry {
			inode: 0,
			entry_type: node.get_file_type(),
			name: Cow::Borrowed(name),
		}))
	}

	fn next_entry(
		&self,
		_inode: INode,
		_fs: &dyn Filesystem,
		off: u64,
	) -> EResult<Option<(DirEntry<'static>, u64)>> {
		let Some((name, node)) = Self::ENTRIES.get(off) else {
			return Ok(None);
		};
		Ok(Some((
			DirEntry {
				inode: 0,
				entry_type: node.get_file_type(),
				name: Cow::Borrowed(name),
			},
			off + 1,
		)))
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test_case]
	fn content_chunks() {
		let chunks = ["abc", "def", "ghi"];
		// Simple test
		let mut out = [0u8; 9];
		let len = super::content_chunks(0, &mut out, chunks.iter().map(Ok)).unwrap();
		assert_eq!(out.as_slice(), b"abcdefghi");
		assert_eq!(len, 9);
		// End
		let mut out = [0u8; 9];
		let len = super::content_chunks(9, &mut out, chunks.iter().map(Ok)).unwrap();
		assert_eq!(out, [0u8; 9]);
		assert_eq!(len, 0);
		// Start from second chunk
		let mut out = [0u8; 9];
		let len = super::content_chunks(3, &mut out, chunks.iter().map(Ok)).unwrap();
		assert_eq!(out.as_slice(), b"defghi\0\0\0");
		assert_eq!(len, 6);
		// Start from middle of chunk
		let mut out = [0u8; 9];
		let len = super::content_chunks(4, &mut out, chunks.iter().map(Ok)).unwrap();
		assert_eq!(out.as_slice(), b"efghi\0\0\0\0");
		assert_eq!(len, 5);
	}
}
