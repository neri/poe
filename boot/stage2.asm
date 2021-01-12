;; MEG-OS Lite Second loader
;; Copyright (c) 2021 MEG-OS project

%define IPL_SIGN        0x1eaf

%define ORG_BASE        0x0800
%define SYSTEM_TABLE    0x0700

%include "osz.inc"


[BITS 16]
[ORG ORG_BASE]

_HEAD:
    db "EM"
    jmp short _crt0

file_size	dw 0
offset_data	dw 0
offset_ramd	dw 0
size_ramd	dw 0

_next:
    hlt
    jmp $

    alignb 16
_END_RESIDENT:

forever:
    jmp short forever

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

    mov di, SYSTEM_TABLE
    mov bp, di
    xor ax, ax
    mov cx, OSZ_SYSTBL_SIZE / 2
    rep stosw

    pop word [bp + OSZ_SYSTBL_ARCH]

    push es
    mov ax, _next
    push ax

    ; DETECT MEMORY SIZE
_DETECT_MEMSZ:
    mov al, [bp + OSZ_SYSTBL_ARCH]
    cmp al, OSZ_ARCH_NEC98
    jz .nec98
    cmp al, OSZ_ARCH_FMT
    jz .fmt

    int 0x12
    mov cl, 6
    shl ax, cl
    mov [bp + OSZ_SYSTBL_MEMSZ], ax
    jmp short .next

.nec98:
    mov al, [es:0x0501]
    and ax, byte 0x07
    inc ax
    mov cl, 13
    shl ax, cl
    mov [bp + OSZ_SYSTBL_MEMSZ], ax
    jmp short .next

.fmt:
    mov ax, 0xC000 ; TOWNS always >1MB
    mov [bp + OSZ_SYSTBL_MEMSZ], ax

.next:

    ; DETECT CPU
_DETECT_CPUID:
    xor si, si

    ; 186?
    mov cx, 0x0121
    shl ch, cl
    jz short .end_cpu
    inc si

    ; 286?
    mov dx,0xF000
    pushf
    pop ax
    mov cx, ax
    and ax, 0x0FFF
    push ax
    popf
    pushf
    pop ax
    and ax,dx
    cmp ax,dx
    jz short .end_cpu
    inc si

    ; 386?
    or cx,dx
    push cx
    popf
    pushf
    pop ax
    and ax,dx
    jz short .end_cpu
    inc si

    ; 486?
    pushfd
    pop eax
    mov ecx, eax
    xor eax, 0x00040000
    push eax
    popfd
    pushfd
    pop eax
    cmp eax, ecx
    jz .end_cpu
    inc si

    ; cpuid?
    mov eax, ecx
    xor eax, 0x00200000
    push eax
    popfd
    pushfd
    pop eax
    xor eax, ecx
    jz .end_cpu
    inc si

    ; amd64?
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000000
    jbe .env_no_amd64
    mov eax, 0x80000001
    cpuid
    bt edx, 29
    jnc short .env_no_amd64
    inc si

.env_no_amd64:
.end_cpu:
    mov ax, si
    mov [bp + OSZ_SYSTBL_CPUID], al

    retf


    align 16
_END:
