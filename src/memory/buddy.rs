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
use crate::memory::NULL;
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
	// TODO free_list: [; MAX_ORDER + 1],
}

/*
 * Structure representing the metadata for a frame of physical memory.
 */
struct Frame {
	// TODO
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
	let z = unsafe { ZONES.get_mut() };

	let kernel_zone_begin = mmap_info.phys_alloc_begin as *mut Void;
	let available_memory_end = (mmap_info.phys_alloc_begin as usize) + mmap_info.available_memory;
	let kernel_zone_end = min(available_memory_end, KERNEL_ZONE_LIMIT as usize) as *mut Void;
	let kernel_zone_size = (kernel_zone_end as usize) - (mmap_info.phys_alloc_begin as usize);
	z[1].lock().get_mut().init(FLAG_ZONE_TYPE_KERNEL, kernel_zone_begin, kernel_zone_size);
	z[1].unlock();

	let user_zone_begin = kernel_zone_end;
	let user_zone_end = available_memory_end as *mut Void;
	let user_zone_size = (user_zone_end as usize) - (user_zone_begin as usize);
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
	let zones = unsafe { ZONES.get_mut() };

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
	let zones = unsafe { ZONES.get_mut() };

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
			zone.frame_mark_used(f, order);
			return Ok(f.get_ptr());
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
pub fn free(_ptr: *const Void, order: FrameOrder) {
	debug_assert!(order <= MAX_ORDER);

	// TODO
}

/*
 * Returns the total number of pages allocated by the buddy allocator.
 */
pub fn allocated_pages() -> usize {
	let mut n = 0;

	unsafe {
		let z = ZONES.get_mut();
		for i in 0..z.len() {
			let guard = MutexGuard::new(&mut z[i]); // TODO Remove `mut`?
			n += guard.get().get_allocated_pages();
		}
	}
	n
}

impl Zone {
	/*
	 * Initializes the zone with type `type_`. The zone covers the memory from pointer `begin` to
	 * `begin + size` where `size` is the size in bytes.
	 */
	pub fn init(&mut self, type_: Flags, begin: *mut Void, size: usize) {
		self.type_ = type_;
		self.allocated_pages = 0;
		self.begin = begin;
		self.size = size;
	}

	/*
	 * Returns the number of allocated pages in the current zone of memory.
	 */
	pub fn get_allocated_pages(&self) -> usize {
		self.allocated_pages
	}

	/*
	 * Returns an available frame owned by this zone, with an order of at least `_order`.
	 */
	pub fn get_available_frame(&self, _order: FrameOrder) -> Option<&'static mut Frame> {
		// TODO
		None
	}

	/*
	 * Splits the given `frame` if larger than `order` until it reaches the said order, inserting
	 * subsequent frames into the free list. The frame is then removed from the free list.
	 */
	pub fn frame_mark_used(&mut self, _frame: &mut Frame, _order: FrameOrder) {
		// TODO
	}

	/*
	 * Marks the given `frame` as free. Then the frame is merged with its buddies recursively if
	 * available.
	 */
	pub fn frame_mark_free(&mut self, _frame: &mut Frame) {
		// TODO
	}
}

impl Frame {
	/*
	 * Returns the pointer to the location of the associated physical memory.
	 */
	pub fn get_ptr(&self) -> *mut Void {
		// TODO
		NULL as _
	}
}
