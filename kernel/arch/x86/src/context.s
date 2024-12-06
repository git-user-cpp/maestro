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

.intel_syntax noprefix
.include "arch/x86/src/regs.s"
.section .text

.macro ERROR id
.global error\id
.type error\id, @function

error\id:
	cld
	push 0 # code (absent)
	push \id
STORE_REGS

	xor ebp, ebp
	push esp
	call interrupt_handler
	add esp, 4

LOAD_REGS
	add esp, 8
	iretd
.endm

.macro ERROR_CODE id
.global error\id
.type error\id, @function

error\id:
	cld
	push \id
STORE_REGS

	xor ebp, ebp
	push esp
	call interrupt_handler
	add esp, 4

LOAD_REGS
	add esp, 8
	iretd
.endm

.macro IRQ id
.global irq\id
.type irq\id, @function

irq\id:
	cld
	push 0 # code (absent)
	push (0x20 + \id)
STORE_REGS

	xor ebp, ebp
	push esp
	call interrupt_handler
	add esp, 4

LOAD_REGS
	add esp, 8
	iretd
.endm

ERROR 0
ERROR 1
ERROR 2
ERROR 3
ERROR 4
ERROR 5
ERROR 6
ERROR 7
ERROR_CODE 8
ERROR 9
ERROR_CODE 10
ERROR_CODE 11
ERROR_CODE 12
ERROR_CODE 13
ERROR_CODE 14
ERROR 15
ERROR 16
ERROR_CODE 17
ERROR 18
ERROR 19
ERROR 20
ERROR 21
ERROR 22
ERROR 23
ERROR 24
ERROR 25
ERROR 26
ERROR 27
ERROR 28
ERROR 29
ERROR_CODE 30
ERROR 31

IRQ 0
IRQ 1
IRQ 2
IRQ 3
IRQ 4
IRQ 5
IRQ 6
IRQ 7
IRQ 8
IRQ 9
IRQ 10
IRQ 11
IRQ 12
IRQ 13
IRQ 14
IRQ 15

.global init_ctx
.type init_ctx, @function

init_ctx:
	# Set user data segment
	mov ax, 0x23
	mov es, ax
	mov ds, ax
	mov esp, [esp + 4]
	LOAD_REGS
	add esp, 8
	iretd

.global syscall
.type syscall, @function

syscall:
	cld
	push 0 # code (absent)
	push 0 # interrupt ID (absent)
STORE_REGS

	xor ebp, ebp
	push esp
	call syscall_handler
	add esp, 4

LOAD_REGS
	add esp, 8
	iretd
