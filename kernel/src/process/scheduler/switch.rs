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

//! Context switching utilities.

use crate::{
	arch::x86::{fxrstor, fxsave, gdt, idt::IntFrame, tss::TSS},
	process::Process,
};
use core::{arch::global_asm, mem::offset_of};

/// Stashes current segment values during execution of `f`, restoring them after.
pub fn stash_segments<F: FnOnce() -> T, T>(f: F) -> T {
	#[cfg(not(target_arch = "x86_64"))]
	{
		f()
	}
	#[cfg(target_arch = "x86_64")]
	{
		use crate::arch::x86;
		let fs_base = x86::rdmsr(x86::IA32_FS_BASE);
		let gs_base = x86::rdmsr(x86::IA32_GS_BASE);
		let kernel_gs_base = x86::rdmsr(x86::IA32_KERNEL_GS_BASE);
		let res = f();
		x86::wrmsr(x86::IA32_FS_BASE, fs_base);
		x86::wrmsr(x86::IA32_GS_BASE, gs_base);
		x86::wrmsr(x86::IA32_KERNEL_GS_BASE, kernel_gs_base);
		res
	}
}

/// Switches context from `prev` to `next`.
///
/// After returning, the execution will continue on `next`.
///
/// # Safety
///
/// The pointers must point to valid processes.
pub unsafe fn switch(prev: *const Process, next: *const Process) {
	stash_segments(|| switch_asm(prev, next));
}

// Note: the functions below are saving only the registers that are not clobbered by the call to
// them

extern "C" {
	/// Jumps to a new context with the given `frame`.
	///
	/// # Safety
	///
	/// The context described by `frame` must be valid.
	pub fn init_ctx(frame: &IntFrame) -> !;
	/// Saves state of the current context in `parent`, then switches to the next context described
	/// by `frame`.
	///
	/// # Safety
	///
	/// The context described by `frame` must be valid.
	#[allow(improper_ctypes)]
	pub fn fork_asm(frame: &IntFrame, parent: *const Process);

	#[allow(improper_ctypes)]
	fn switch_asm(prev: *const Process, next: *const Process);
}

#[cfg(target_arch = "x86")]
global_asm!(r#"
.section .text

.global fork_asm
.global switch_asm
.type fork_asm, @function
.type switch_asm, @function

fork_asm:
	# Save parent context
	push ebp
	push ebx
	push esi
	push edi
    mov eax, [esp + 24]
    mov [eax + {off}], esp

	# Set stack at the frame's position
	add esp, 16
	jmp init_ctx

switch_asm:
	push ebp
	push ebx
	push esi
	push edi

    # Swap contexts
    mov eax, [esp + 20]
    mov [eax + {off}], esp
    mov eax, [esp + 24]
    mov esp, [eax + {off}]

	pop edi
	pop esi
	pop ebx
	pop ebp

	jmp switch_finish
"#, off = const offset_of!(Process, kernel_sp));

#[cfg(target_arch = "x86_64")]
global_asm!(r#"
.section .text

.global fork_asm
.global switch_asm
.type fork_asm, @function
.type switch_asm, @function

fork_asm:
	# Save parent context, to resume in `switch_asm`
	push rbp
	push rbx
	push r12
	push r13
	push r14
	push r15
	push fs
	push gs
    mov [rsi + {off}], rsp

	jmp init_ctx

switch_asm:
	push rbp
	push rbx
	push r12
	push r13
	push r14
	push r15
	push fs
	push gs

    # Swap contexts
    mov [rdi + {off}], rsp
    mov rsp, [rsi + {off}]

	pop gs
	pop fs
	pop r15
	pop r14
	pop r13
	pop r12
	pop rbx
	pop rbp

	jmp switch_finish
"#, off = const offset_of!(Process, kernel_sp));

/// Finishes switching context from `prev` to `next`, that is restore everything else than
/// general-purpose registers.
///
/// This function is jumped to from [`switch`].
#[export_name = "switch_finish"]
pub extern "C" fn finish(prev: &Process, next: &Process) {
	// Bind the memory space
	next.mem_space.as_ref().unwrap().lock().bind();
	// Update the TSS for the process
	unsafe {
		TSS.set_kernel_stack(next.kernel_stack_top());
	}
	// Update TLS entries in the GDT
	{
		let tls = next.tls.lock();
		for (i, ent) in tls.iter().enumerate() {
			unsafe {
				ent.update_gdt(gdt::TLS_OFFSET + i * size_of::<gdt::Entry>());
			}
		}
	}
	// TODO save and restore only if necessary (enable the FPU when the first interruption occurs)
	// Save and restore FPU state
	fxsave(&mut prev.fpu.lock());
	fxrstor(&next.fpu.lock());
}
