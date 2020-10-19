/*
 * This module contains the buddy allocator which allows to allocate 2^^n pages
 * large frames of memory.
 *
 * This allocator works by dividing frames of memory in two until the a frame of
 * the required size is available.
 *
 * The order of a frame is the `n` in the expression `2^^n` that represents the
 * size of a frame in pages.
 */

use core::cmp::min;
use core::mem::MaybeUninit;
use crate::memory::Void;
use crate::memory::memmap;
use crate::memory;
use crate::util::lock::Mutex;
use crate::util::lock::MutexGuard;
use crate::util;

/*
 * Type representing the order of a memory frame.
 */
pub type FrameOrder = u8;
/*
 * Type representing buddy allocator flags.
 */
pub type Flags = i32;
/*
 * Type representing the identifier of a frame.
 */
type FrameID = u32;

/*
 * The maximum order of a buddy allocated frame.
 */
pub const MAX_ORDER: FrameOrder = 17;

/*
 * The mask for the type of the zone in buddy allocator flags.
 */
const ZONE_TYPE_MASK: Flags = 0b11;
/*
 * Buddy allocator flag. Tells that the allocated frame must be mapped into the user zone.
 */
pub const FLAG_ZONE_TYPE_USER: Flags = 0b000;
/*
 * Buddy allocator flag. Tells that the allocated frame must be mapped into the kernel zone.
 */
pub const FLAG_ZONE_TYPE_KERNEL: Flags = 0b001;
/*
 * Buddy allocator flag. Tells that the allocated frame must be mapped into the DMA zone.
 */
pub const FLAG_ZONE_TYPE_DMA: Flags = 0b010;
/*
 * Buddy allocator flag. Tells that the allocation shall not fail (unless not enough memory is
 * present on the system). This flag is ignored if FLAG_USER is not specified or if the allocation
 * order is higher than 0. The allocator shall use the OOM killer to recover memory.
 */
pub const FLAG_NOFAIL: Flags = 0b100;

/*
 * Pointer to the end of the kernel zone of memory with the maximum possible size.
 */
pub const KERNEL_ZONE_LIMIT: *const Void = 0x40000000 as _;

/*
 * Value indicating that the frame is used.
 */
pub const FRAME_STATE_USED: FrameID = !(0 as FrameID);

// TODO OOM killer

/*
 * Structure representing an allocatable zone of memory.
 */
struct Zone {
	/* The type of the zone, defining the priority */
	type_: Flags,
	/* The number of allocated pages in the zone */
	allocated_pages: usize,

	/* A pointer to the beginning of the zone */
	begin: *mut Void,
	/* The size of the zone in bytes */
	size: usize,

	/* The free list containing linked lists to free frames */
	free_list: [Option<*mut Frame>; (MAX_ORDER + 1) as usize],
}

/*
 * Structure representing the metadata for a frame of physical memory. The structure has an internal linked list for the
 * free list. This linked list doesn't store pointers but frame identifiers to save memory. If either `prev` or `next`
 * has value `FRAME_STATE_USED`, the frame is marked as used. If a frame points to itself, it means that no more
 * elements are present in the list.
 */
#[repr(packed)]
struct Frame {
	/* Identifier of the previous frame in the free list. */
	prev: FrameID,
	/* Identifier of the next frame in the free list. */
	next: FrameID,
	/* Order of the current frame */
	order: FrameOrder,
}

// TODO Remplace by a linked list? (in case of holes in memory)
/*
 * The array of buddy allocator zones.
 */
static mut ZONES: MaybeUninit<[Mutex<Zone>; 3]> = MaybeUninit::uninit();

/*
 * The size in bytes of a frame allocated by the buddy allocator with the given `order`.
 */
pub fn get_frame_size(order: FrameOrder) -> usize {
	memory::PAGE_SIZE << order
}

/*
 * Returns the buddy order required to fit the given number of pages.
 */
pub fn get_order(pages: usize) -> FrameOrder {
	let mut order: FrameOrder = 0;
	let mut i = 1;

	while i < pages {
		i *= 2;
		order += 1;
	}
	order
}

/*
 * Initializes the buddy allocator.
 */
pub fn init() {
	unsafe {
		util::zero_object(&mut ZONES);
	}

	let mmap_info = memmap::get_info();
	let z = unsafe {
		ZONES.assume_init_mut()
	};

	let kernel_zone_begin = memory::kern_to_virt(mmap_info.phys_alloc_begin) as *mut Void;
	let available_memory_end = (mmap_info.phys_alloc_begin as usize) + mmap_info.available_memory;
	let kernel_zone_end = min(available_memory_end, KERNEL_ZONE_LIMIT as usize) as *mut Void;
	let kernel_zone_size = (kernel_zone_end as usize) - (mmap_info.phys_alloc_begin as usize);
	z[1].lock().get_mut().init(FLAG_ZONE_TYPE_KERNEL, kernel_zone_begin, kernel_zone_size);
	z[1].unlock();

	// TODO
	/*let user_zone_begin = kernel_zone_end;
	let user_zone_end = available_memory_end as *mut Void;
	let user_zone_size = (user_zone_end as usize) - (user_zone_begin as usize);*/
	let user_zone_begin = 0 as *mut _;
	let user_zone_size = 0;
	z[0].lock().get_mut().init(FLAG_ZONE_TYPE_USER, user_zone_begin, user_zone_size);
	z[0].unlock();

	// TODO
	z[2].lock().get_mut().init(FLAG_ZONE_TYPE_DMA, 0 as *mut _, 0);
	z[2].unlock();
}

// TODO Allow to fallback to another zone if the one that is returned is full
/*
 * Returns a mutable reference to a zone suitable for an allocation with the given type `type_`.
 */
fn get_suitable_zone(type_: Flags) -> Option<&'static mut Mutex<Zone>> {
	let zones = unsafe { ZONES.assume_init_mut() };

	for i in 0..zones.len() {
		let is_valid = {
			let guard = MutexGuard::new(&mut zones[i]);
			let zone = guard.get();
			zone.type_ == type_
		};
		if is_valid {
			return Some(&mut zones[i]);
		}
	}
	None
}

/*
 * Returns a mutable reference to the zone that contains the given pointer.
 */
fn get_zone_for_pointer(ptr: *const Void) -> Option<&'static mut Mutex<Zone>> {
	let zones = unsafe { ZONES.assume_init_mut() };

	for i in 0..zones.len() {
		let is_valid = {
			let guard = MutexGuard::new(&mut zones[i]);
			let zone = guard.get();
			ptr >= zone.begin && (ptr as usize) < zone.begin as usize + zone.size
		};
		if is_valid {
			return Some(&mut zones[i]);
		}
	}
	None
}

/*
 * Allocates a frame of memory using the buddy allocator.
 */
pub fn alloc(order: FrameOrder, flags: Flags) -> Result<*mut Void, ()> {
	debug_assert!(order <= MAX_ORDER);

	let z = get_suitable_zone(flags & ZONE_TYPE_MASK);
	if let Some(z_) = z {
		let mut guard = MutexGuard::new(z_);
		let zone = guard.get_mut();

		let frame = zone.get_available_frame(order);
		if let Some(f) = frame {
			f.split(zone, order);
			f.mark_used();
			let ptr = f.get_ptr(zone);
			debug_assert!(util::is_aligned(ptr, memory::PAGE_SIZE));
			return Ok(ptr);
		}
	}
	Err(())
}

/*
 * Uses `alloc` and zeroes the allocated frame.
 */
pub fn alloc_zero(order: FrameOrder, flags: Flags) -> Result<*mut Void, ()> {
	let ptr = alloc(order, flags)?;
	let len = get_frame_size(order);
	unsafe {
		util::bzero(ptr, len);
	}
	Ok(ptr)
}

/*
 * Frees the given memory frame that was allocated using the buddy allocator. The given order must
 * be the same as the one given to allocate the frame.
 */
pub fn free(ptr: *const Void, order: FrameOrder) {
	debug_assert!(util::is_aligned(ptr, memory::PAGE_SIZE));
	debug_assert!(order <= MAX_ORDER);

	let z = get_zone_for_pointer(ptr);
	if let Some(z_) = z {
		let mut guard = MutexGuard::new(z_);
		let zone = guard.get_mut();

		let frame_id = zone.get_frame_id_from_ptr(ptr);
		debug_assert!(frame_id < zone.get_pages_count());
		let frame = zone.get_frame(frame_id);
		unsafe {
			(*frame).mark_free();
			(*frame).coalesce(zone);
		}
	}
}

/*
 * Returns the total number of pages allocated by the buddy allocator.
 */
pub fn allocated_pages() -> usize {
	let mut n = 0;

	unsafe {
		let z = ZONES.assume_init_mut();
		for i in 0..z.len() {
			let guard = MutexGuard::new(&mut z[i]); // TODO Remove `mut`?
			n += guard.get().get_allocated_pages();
		}
	}
	n
}

impl Zone {
	/*
	 * Fills the free list during initialization according to the number of available pages.
	 */
	fn fill_free_list(&mut self) {
		let pages_count = self.get_pages_count();
		let mut frame: FrameID = 0;
		let mut order = MAX_ORDER;

		while frame < pages_count as FrameID {
			let p = util::pow2(order as _) as FrameID;
			if frame + p > pages_count {
				if order == 0 {
					break;
				}
				order -= 1;
				continue;
			}

			let f = unsafe { &mut *self.get_frame(frame) };
			f.mark_free();
			f.order = order;
			f.link(self);

			frame += p;
		}
	}

	/*
	 * Initializes the zone with type `type_`. The zone covers the memory from pointer `begin` to
	 * `begin + size` where `size` is the size in bytes.
	 */
	pub fn init(&mut self, type_: Flags, begin: *mut Void, size: usize) {
		self.type_ = type_;
		self.allocated_pages = 0;
		self.begin = begin;
		self.size = size;
		self.fill_free_list();
	}

	/*
	 * Returns the number of allocated pages in the current zone of memory.
	 */
	pub fn get_allocated_pages(&self) -> usize {
		self.allocated_pages
	}

	/*
	 * Returns the pointer to the beginning of the allocatable zone, after the metadata zone.
	 */
	pub fn get_data_begin(&self) -> *mut Void {
		let frames_count = (self.get_pages_count() as usize) * core::mem::size_of::<Frame>();
		util::align(((self.begin as usize) + frames_count) as _, memory::PAGE_SIZE) as _
	}

	/*
	 * Returns the number of allocatable pages.
	 */
	pub fn get_pages_count(&self) -> FrameID {
		(self.size / (memory::PAGE_SIZE + core::mem::size_of::<Frame>())) as _
	}

	/*
	 * Returns an available frame owned by this zone, with an order of at least `order`.
	 */
	pub fn get_available_frame(&self, order: FrameOrder) -> Option<&'static mut Frame> {
		for i in (order as usize)..self.free_list.len() {
			let f = self.free_list[i];
			if let Some(f_) = f {
				return Some(unsafe { &mut *f_ });
			}
		}
		None
	}

	/*
	 * Returns the identifier for the frame at the given pointer `ptr`. The pointer must point to
	 * the frame itself, not the Frame structure.
	 */
	pub fn get_frame_id_from_ptr(&self, ptr: *const Void) -> FrameID {
		let frame_size = core::mem::size_of::<Frame>() + memory::PAGE_SIZE;
		(((ptr as usize) - (self.begin as usize)) / frame_size) as _
	}

	/*
	 * Returns a mutable reference to the frame with the given identifier `id`.
	 * The given identifier **must** be in the range of the zone.
	 */
	pub fn get_frame(&mut self, id: FrameID) -> *mut Frame {
		debug_assert!(id < self.get_pages_count());
		let off = (self.begin as usize) + (id as usize * core::mem::size_of::<Frame>());
		off as _
	}
}

impl Frame {
	/*
	 * Returns the id of the current frame in the associated zone `zone`.
	 */
	pub fn get_id(&self, zone: &Zone) -> FrameID {
		let self_off = self as *const _ as usize;
		let zone_off = zone.begin as *const _ as usize;
		debug_assert!(self_off >= zone_off);
		((self_off - zone_off) / core::mem::size_of::<Self>()) as u32
	}

	/*
	 * Returns the pointer to the location of the associated physical memory.
	 */
	pub fn get_ptr(&self, zone: &Zone) -> *mut Void {
		let begin_offset = zone.begin as *const _ as usize;
		let mut data_begin = begin_offset
			+ zone.get_pages_count() as usize * core::mem::size_of::<Self>();
		data_begin = util::align(data_begin as _, memory::PAGE_SIZE) as _;

		debug_assert!((data_begin - begin_offset)
			+ zone.get_pages_count() as usize * memory::PAGE_SIZE <= zone.size);

		let off = self.get_id(zone) as usize * memory::PAGE_SIZE;
		(data_begin + off) as _
	}

	/*
	 * Tells whether the frame is used or not.
	 */
	pub fn is_used(&self) -> bool {
		(self.prev == FRAME_STATE_USED) || (self.next == FRAME_STATE_USED)
	}

	/*
	 * Marks the frame as used. The frame must not be linked to any free list.
	 */
	pub fn mark_used(&mut self) {
		self.prev = FRAME_STATE_USED;
	}

	/*
	 * Marks the frame as free. The frame must not be linked to any free list.
	 */
	pub fn mark_free(&mut self) {
		self.prev = 0;
	}

	/*
	 * Returns the identifier of the buddy frame in zone `zone`, taking in account the frame's
	 * order.
	 * The return value might be invalid and the caller has the reponsibility to check that it is
	 * below the number of frames in the zone.
	 */
	pub fn get_buddy_id(&self, zone: &Zone) -> FrameID {
		self.get_id(zone) ^ (1 << self.order) as u32
	}

	/*
	 * Links the frame into zone `zone`'s free list.
	 */
	pub fn link(&mut self, zone: &mut Zone) {
		debug_assert!(!self.is_used());

		let id = self.get_id(zone);
		self.prev = id;
		self.next = if let Some(n) = zone.free_list[self.order as usize] {
			unsafe {
				(*n).get_id(zone)
			}
		} else {
			id
		};
		zone.free_list[self.order as usize] = Some(self);
	}

	/*
	 * Unlinks the frame from zone `zone`'s free list.
	 */
	pub fn unlink(&mut self, zone: &mut Zone) {
		debug_assert!(!self.is_used());

		let id = self.get_id(zone);
		let has_prev = self.prev != id;
		let has_next = self.next != id;
		if has_prev {
			let prev = zone.get_frame(self.prev);
			unsafe {
				(*prev).next = if has_next { self.next } else { self.prev };
			}
		} else {
			zone.free_list[self.order as usize] = if has_next {
				Some(zone.get_frame(self.next))
			} else {
				None
			}
		}

		if has_next {
			let next = zone.get_frame(self.next);
			unsafe {
				(*next).prev = if has_prev { self.prev } else { self.next };
			}
		}
	}

	/*
	 * Unlinks the frame from zone `zone`'s free list, splits it until it reaches the required
	 * order `order` while linking the new free frames to the free list. At the end of the
	 * function, the current frame is **not** linked to the free list.
	 *
	 * The frame must not be marked as used.
	 */
	pub fn split(&mut self, zone: &mut Zone, order: FrameOrder) {
		debug_assert!(!self.is_used());
		debug_assert!(self.order >= order);

		self.unlink(zone);
		while self.order > order {
			self.order -= 1;

			let buddy = self.get_buddy_id(zone);
			if buddy >= zone.get_pages_count() {
				break;
			}

			let buddy_frame = unsafe { &mut *zone.get_frame(buddy) };
			debug_assert!(!buddy_frame.is_used());
			buddy_frame.unlink(zone);
			buddy_frame.order = self.order;
			buddy_frame.link(zone);
		}
	}

	/*
	 * Coealesces the frame in zone `zone` with free buddy blocks recursively until no buddy is
	 * available anymore. Buddies that are merges with the frame are unlinked. The order of the
	 * frame is incremented at each merge. The frame is linked to the free list at the end.
	 *
	 * The frame must not be marked as used.
	 */
	pub fn coalesce(&mut self, zone: &mut Zone) {
		debug_assert!(!self.is_used());

		while self.order < MAX_ORDER {
			let buddy = self.get_buddy_id(zone);
			if buddy >= zone.get_pages_count() {
				break;
			}

			let buddy_frame = unsafe { &mut *zone.get_frame(buddy) };
			if buddy_frame.order != self.order && !buddy_frame.is_used() {
				break;
			}

			buddy_frame.unlink(zone);
			self.order += 1;
		}
		self.link(zone);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test_case]
	fn test_buddy0() {
		if let Ok(p) = alloc(0, FLAG_ZONE_TYPE_KERNEL) {
			unsafe {
				util::memset(p, -1, get_frame_size(0));
			}
			free(p, 0);
		} else {
			assert!(false);
		}
	}

	#[test_case]
	fn test_buddy1() {
		for _ in 0..1000 {
			if let Ok(p) = alloc(0, FLAG_ZONE_TYPE_KERNEL) {
				unsafe {
					util::memset(p, -1, get_frame_size(0));
				}
				free(p, 0);
			} else {
				assert!(false);
			}
		}
	}

	#[test_case]
	fn test_buddy2() {
		if let Ok(p) = alloc(1, FLAG_ZONE_TYPE_KERNEL) {
			unsafe {
				util::memset(p, -1, get_frame_size(1));
			}
			free(p, 1);
		} else {
			assert!(false);
		}
	}

	fn test3(i: usize) {
		if let Ok(p) = alloc(0, FLAG_ZONE_TYPE_KERNEL) {
			unsafe {
				util::memset(p, -1, get_frame_size(0));
			}
			if i > 0 {
				test3(i - 1);
			}
			free(p, 0);
		} else {
			assert!(false);
		}
	}

	#[test_case]
	fn test_buddy3() {
		test3(100);
	}

	// TODO Add more tests
}
