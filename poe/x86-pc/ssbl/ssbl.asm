;;  Second Stage Boot Loader
;;
;;  MIT License
;;
;;  Copyright (c) 2021 MEG-OS project
;;
;;  Permission is hereby granted, free of charge, to any person obtaining a copy
;;  of this software and associated documentation files (the "Software"), to deal
;;  in the Software without restriction, including without limitation the rights
;;  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
;;  copies of the Software, and to permit persons to whom the Software is
;;  furnished to do so, subject to the following conditions:
;;
;;  The above copyright notice and this permission notice shall be included in all
;;  copies or substantial portions of the Software.
;;
;;  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
;;  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
;;  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
;;  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
;;  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
;;  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
;;  SOFTWARE.
;;

%define IPL_SIGN            0x1eaf
%define PF_NEC98            1       ; NEC PC-98 Series Computer
%define PF_PC               2       ; IBM PC Compatible
%define PF_FMT              3       ; Fujitsu FM TOWNS

%define ORG_BASE            0x0800

%define KERNEL_CS           0x08
%define KERNEL_DS           0x10
%define STACK_SIZE          0x1000

%define CEEF_MAGIC_V1       0x0001ceef
%define CEEF_SKIP_DATA      16

%define CEEF_ENTRY          0x04
%define CEEF_BASE           0x08
%define CEEF_MINALLOC       0x0C

%define MAX_PALETTE         16

%define SMAP_AVAILABLE      0x01
%define SMAP_RESERVED       0x02
%define SMAP_ACPI_RECLAIM   0x03
%define SMAP_ACPI_NVS       0x04

[BITS 16]
[ORG ORG_BASE]

_HEAD:
    jmp short _crt0
    dw _END - _HEAD

forever:
    sti
    hlt
    jmp forever

_crt0:
    xor ax, IPL_SIGN
    jnz forever

    mov es, ax
    mov ss, ax
    mov sp, ORG_BASE
    push cx
    push ax
    popf
    push cs
    pop ds

    xor si, si
    mov di, ORG_BASE
    mov cx,  (_END - _HEAD)/2
    rep movsw

    ;; save old CS
    push cs

    jmp 0:_init

_puts:
    mov al, [cs:_platform]
    cmp al, PF_NEC98
    jz _puts_nec98
    cmp al, PF_FMT
    jz _puts_fmt
.loop:
    lodsb
    or al, al
    jz .end
    mov ah, 0x0e
    int 0x10
    jmp .loop
.end:
    ret

_puts_fmt:
    push es
    pusha
    sub si, 3
    mov bh, 0x02
    call 0xfffb:0x001e
    popa
    pop es
    ret

_puts_nec98:
    push es
    mov ax, 0xa000
    mov es, ax
    xor di, di
    mov cl, [si - 1]
    xor ch, ch
    xor ah, ah
.loop:
    lodsb
    stosw
    mov al, 0xe1
    mov [es:di + 0x1ffe], ax
    loop .loop
    pop es
    ret


_ps2_wait_for_write:
    in al, 0x64
    test al, 2
    jnz _ps2_wait_for_write
    ret

_ps2_wait_for_read:
    in al, 0x64
    test al, 1
    jz _ps2_wait_for_read
    ret


_init:
    push cs
    pop ds

    pop es
    pop ax
    inc ax
    mov [_platform], ax
    push es

    ;; check cpu
_check_cpu:
    ;; is 286 or later
    mov dx, 0xf000
    pushf
    pop ax
    mov cx, ax
    and ax, 0x0fff
    push ax
    popf
    pushf
    pop ax
    and ax, dx
    cmp ax, dx
    jnz short .286_ok
.bad_cpu:
    mov si, cpu_err_mes
    call _puts
    jmp forever

.286_ok:
    ;; is 386 or later
    or cx, dx
    push cx
    popf
    pushf
    pop ax
    and ax, dx
    jz short .bad_cpu

.cpu_ok:

_mem_check:
    mov al, [_platform]
    cmp al, PF_PC
    jz _init_pc
    cmp al, PF_FMT
    jz _init_fmt

_init_n98:
    mov al, [0x0501]
    and ax, byte 0x07
    inc ax
    mov cl, 13
    shl ax, cl
    mov [_memsz_lo], ax

    movzx eax, byte [0x0401]
    mov cl, 17
    shl eax, cl
    mov [_memsz_mid], eax

    ;; A20
    xor al, al
    out 0xf2, al

    jmp _end_mem_check

_init_fmt:
    mov ax, 0xc000 ; TOWNS always > 768KB
    mov [_memsz_lo], ax

    mov dx, 0x05e8
    in al, dx
    dec al
    and eax, 0x7f
    shl eax, 20
    mov [_memsz_mid], eax

    jmp _end_mem_check

_init_pc:
    int 0x12
    mov cl, 6
    shl ax, cl
    mov [_memsz_lo], ax

    mov ah, 0x88
    stc
    int 0x15
    jc .no_1588
    movzx eax, ax
    shl eax, 10
    mov [_memsz_mid], eax
.no_1588:

    push es
    sub sp, 20
    xor ebx, ebx
    mov es, bx
    mov di, sp
    ; mov si, _smap
.loop:
    mov eax, 0xe820
    mov edx, 0x534d4150 ; SMAP
    mov ecx, 20
    int 0x15
    jc .end
    cmp eax, 0x534d4150 ; SMAP
    jnz .end
    mov eax, [es:di + 4]
    or eax, [es:di + 12]
    jnz .skip
    mov al, [es:di + 16]
    cmp al, SMAP_AVAILABLE
    jnz .skip
    mov eax, [es:di]
    cmp eax, 0x00100000
    jb .skip
    mov eax, [es:di + 8]
    mov [_memsz_mid], eax
    jmp .end
.skip:
    or ebx, ebx
    jnz .loop
.end:
    add sp, 20
    pop es

    call _a20_check
    jnc .a20_skip

    ;; A20 control
    call _ps2_wait_for_write
    mov al, 0xad
    out 0x64, al

    call _ps2_wait_for_write
    mov al, 0xd0
    out 0x64, al

    call _ps2_wait_for_read
    in al, 0x60
    push ax

    call _ps2_wait_for_write
    mov al, 0xd1
    out 0x64, al

    call _ps2_wait_for_write
    pop ax
    or al, 2
    out 0x60, al

    call _ps2_wait_for_write
    mov al, 0xae
    out 0x64, al

    ; call _ps2_wait_for_write

    in al, 0x92
    or al, 2
    out 0x92, al

.a20_skip:

    jmp _end_mem_check

_a20_err:
    push cs
    pop ds
    mov si, a20_err_mes
    call _puts
    jmp forever

_a20_check:
    push ds
    push es
    pusha
    xor ax, ax
    mov ds, ax
    dec ax
    mov es, ax
    xor si, si
    mov di, 16
; .loop_a20_check:
    mov eax, [si]
    cmp [es:di], eax
    jnz .a20_ok
    mov edx, eax
    not edx
    mov [es:di], edx
    cmp [es:di], eax
    jnz .a20_ok
    stc
    jmp .a20_skip
.a20_ok:
    clc
.a20_skip:
    popa
    pop es
    pop ds
    ret

_end_mem_check:

    ;; memory check (temp)
    cmp word [_memsz_mid + 2], 0x0030
    jae .mem_ok
    mov si, no_mem_mes
    call _puts
    jmp forever
.mem_ok:

    ;; kernel signature check
    lea bx, [_END - _HEAD]
    cmp dword [es:bx], CEEF_MAGIC_V1
    jz .kernel_ok
.bad_magic:
    mov si, bad_magc_mes
    call _puts
    jmp forever
.kernel_ok:

    ;;  A20 check
    call _a20_check
    jc _a20_err

    cli

    ;; restore old CS
    pop cx

    lgdt [_GDT]

    mov eax, cr0
    or eax, byte 1
    mov cr0, eax
    db 0xeb, 0x00 ; JUST IN CASE

    mov ax, KERNEL_DS
    mov ss, ax
    movzx esp, sp
    mov ds, ax
    mov es, ax
    xor ax, ax
    mov fs, ax
    mov gs, ax

    jmp KERNEL_CS:_next32

[BITS 32]
_next32:

    movzx ebp, cx
    shl ebp, 4
    add ebp, _END - _HEAD

    mov edi, [ebp + CEEF_BASE]
    mov ecx, [ebp + CEEF_MINALLOC]
    xor al, al
    rep stosb

    add edi, 0x00000fff
    and edi, 0xfffff000
    add edi, STACK_SIZE
    mov esp, edi

    mov eax, [_start_mid]
    mov [_start_mid], edi
    add eax, [_memsz_mid]
    sub eax, edi
    mov [_memsz_mid], eax

    lea esi, [ebp + CEEF_SKIP_DATA]
    mov edi, [ebp + CEEF_BASE]
    call _tek1_decode

    push byte 0
    popfd
    sub esp, 12
    mov ecx, _boot_info
    push ecx
    call [ebp + CEEF_ENTRY]
    ud2


getnum_s7s:
    xor eax, eax
.l00:
    shl eax, 8
    lodsb
    shr eax, 1
    jnc .l00
    ret


_tek1_decode:
    push ebp

    call getnum_s7s
    xchg eax, ebp
    call getnum_s7s

    add ebp, edi
.loop:
    lodsb
    movzx ebx, al
    and eax, byte 0x0f
    jnz short .skiplong_by0
    call getnum_s7s
.skiplong_by0:
    xchg eax, ecx
    shr ebx, 4
    jnz short .skiplong_lz0
    call getnum_s7s
    xchg eax, ebx
.skiplong_lz0:
    rep movsb
.loop_lz:
    cmp edi, ebp
    jae short .fin

    lodsb
    movzx ecx, al
    and eax, byte 0x0f
    shr eax, 1
    jc short .l022
.l021:
    shl eax, 8
    lodsb
    shr eax, 1
    jnc .l021
.l022:
    xchg eax, edx
    shr ecx, 4
    jnz short .skiplong_cp0
    call getnum_s7s
    xchg eax, ecx
.skiplong_cp0:
    not edx
    inc ecx

.l023:
    mov al, [edi + edx]
    stosb
    loop .l023

    dec ebx
    jnz short .loop_lz
    cmp edi, ebp
    jnae short .loop

.fin:
    xor eax, eax
    pop ebp
    ret


    db 22, 0, 9
cpu_err_mes:
    db "NEEDS 386", 0

    db 22, 0, 9
a20_err_mes:
    db "A20 ERROR", 0

    db 22, 0, 17
no_mem_mes:
    db "NOT ENOUGH MEMORY", 0

    db 22, 0, 13
bad_magc_mes:
    db "BROKEN SYSTEM", 0

    ;; Temporarily GDT
    alignb 8
_GDT:
    dw (_end_GDT - _GDT - 1), _GDT, 0x0000, 0x0000 ;; 00 NULL
    dw 0xffff, 0x0000, 0x9a00, 0x00cf   ;; 08 32bit KERNEL TEXT FLAT
    dw 0xffff, 0x0000, 0x9200, 0x00cf   ;; 10 32bit KERNEL DATA FLAT
_end_GDT:

    ; alignb 16
_boot_info:
_platform           db 0
_boot_drive         db 0
_memsz_lo           dw 0
_reserved_memsz     dd 0x00100000
_start_mid          dd 0x00100000
_memsz_mid          dd 0


    alignb 16
_END:
