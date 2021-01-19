;; MEG-OS ToE Loader
;; Copyright (c) 2021 MEG-OS project

%define IPL_SIGN            0x1eaf

%define KERNEL_CS           0x10
%define KERNEL_DS           0x18

%define ORG_BASE            0x0800

%define CEEF_MAGIC          0xCEEF
%define CEEF_OFF_SECHDR     0x10
%define CEEF_SIZE_SECHDR    0x10

%define CEEF_N_SECS         0x03
%define CEEF_ENTRY          0x04
%define CEEF_BASE           0x08
%define CEEF_MINALLOC       0x0C

%define CEEF_S_FILESZ       0x04
%define CEEF_S_VADDR        0x08
%define CEEF_S_MEMSZ        0x0C

%define OSZ_ARCH_PC         1   ; IBM PC/AT Compatible
%define OSZ_ARCH_NEC98      0   ; NEC PC-98 Series Computer
%define OSZ_ARCH_FMT        2   ; Fujitsu FM TOWNS

%define VESA_MODE           0x4101 ; 640x480x8


[BITS 16]
[ORG ORG_BASE]

_HEAD:
    jmp _crt0

forever:
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
    push ds

    jmp 0:_init

_puts:
.loop:
    lodsb
    or al, al
    jz .end
    mov ah, 0x0E
    int 0x10
    jmp .loop
.end:
    ret


_init:
    push cs
    pop ds

    pop es
    pop cx
    mov [_boot_arch], cx
    push es

    cmp word [es:_END - _HEAD], CEEF_MAGIC
    jz .elf_ok
    mov si, bad_kernel_mes
    call _puts
    jmp forever
.elf_ok:

_set_video_mode:
    sub sp, 256
    push ss
    pop es

    mov ax, 0x4F02
    mov bx, VESA_MODE
    int 0x10
    jc .no_vesa
    or ah, ah
    jnz .no_vesa
    ; mov ax, 0x4F03
    ; xor bx, bx
    ; int 0x10
; .vesa_ok:
    mov ax, 0x4F01
    mov cx, bx
    mov di, sp
    int 0x10
    jc .no_vesa
    or ah, ah
    jnz .no_vesa
    mov eax, [es:di + 0x12]
    mov [_screen_width], eax
    mov ax, [es:di + 0x10]
    mov [_screen_stride], ax
    mov al, [es:di + 0x19]
    mov [_screen_bpp], al
    mov eax, [es:di + 0x28]
    mov [_vram_base],eax

    jmp .next

.no_vesa:
    mov ax,0x0013
    int 0x10
    mov eax, 320 + 200 * 0x10000
    mov [_screen_width], eax
    mov [_screen_stride], ax
    mov dword [_vram_base], 0x000A0000

.next:
    add sp, 256
_next:

    lgdt [_GDT]

    mov eax, cr0
    or eax, byte 1
    mov cr0, eax
    db 0xEB, 0x00 ; just in case

    pop cx
    mov ax, KERNEL_DS
    jmp KERNEL_CS:_next32


[BITS 32]

_next32:
    mov ss, ax
    movzx esp, sp
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    movzx ebp, cx
    shl ebp, 4
    add ebp, _END - _HEAD

    mov edi, [ebp + CEEF_BASE]
    mov ecx, [ebp + CEEF_MINALLOC]
    xor al, al
    rep stosb

    movzx edx, byte [ebp + CEEF_N_SECS]
    lea ebx, [ebp + CEEF_OFF_SECHDR]
    mov esi, edx
    shl esi, 4
    add esi, ebx
.loop:
    mov al, [ebx]
    and al, 0x07
    jz .no_load

    mov ecx, [ebx + CEEF_S_FILESZ]
    jecxz .no_load
    mov edi, [ebx + CEEF_S_VADDR]
    rep movsb

.no_load:
    add ebx, CEEF_SIZE_SECHDR
    dec edx
    jnz .loop

    mov eax, _boot_info
    push eax
    call [ebp + CEEF_ENTRY]
    ud2

cpu_err_mes:
    db "CPU NOT SUPPORTED", 13, 10, 0

no_mem_mes:
    db "NOT ENOUGH MEMORY", 13, 10, 0

bad_kernel_mes:
    db "BAD KERNEL SIGNATURE", 13, 10, 0

_boot_info:
_vram_base      dd 0x000A0000
_screen_width   dw 640
_screen_height  dw 480
_screen_stride  dw 640
_screen_bpp     db 0
                db 0
_boot_arch      db 0
_boot_drive     db 0

    ;;　GDT
    alignb 16
_GDT:
    dw (_end_GDT - _GDT - 1), _GDT, 0x0000, 0x0000 ;; 00 NULL
    dw 0, 0, 0, 0   ;; 08 RESERVED
    dw 0xFFFF, 0x0000, 0x9A00, 0x00CF   ;; 10 32bit KERNEL TEXT FLAT
    dw 0xFFFF, 0x0000, 0x9200, 0x00CF   ;; 18 32bit KERNEL DATA FLAT
_end_GDT:

_END:
