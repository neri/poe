;; TOE Loader
;; Copyright (c) 2021 MEG-OS project

%define IPL_SIGN            0x1eaf
%define ARCH_PC             1   ; IBM PC/AT Compatible
%define ARCH_NEC98          0   ; NEC PC-98 Series Computer
%define ARCH_FMT            2   ; Fujitsu FM TOWNS

%define ORG_BASE            0x0800

%define KERNEL_CS           0x10
%define KERNEL_DS           0x18
%define STACK_SIZE          0x10000

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

%define VESA_MODE           0x4101 ; 640x480x8
%define MAX_PALETTE         256

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
    mov al, [cs:_boot_arch]
    cmp al, ARCH_NEC98
    jz _puts_nec98
    cmp al, ARCH_FMT
    jz _puts_fmt
.loop:
    lodsb
    or al, al
    jz .end
    mov ah, 0x0E
    int 0x10
    jmp .loop
.end:
    ret

_puts_fmt:
    push es
    pusha
    sub si, 3
    mov bh, 0x02
    call 0xFFFB:0x001E
    popa
    pop es
    ret

_puts_nec98:
    push es
    mov ax, 0xA000
    mov es, ax
    xor di, di
    mov cl, [si - 1]
    xor ch, ch
.loop:
    lodsb
    xor ah, ah
    stosw
    mov ax, 0x00E1
    mov [es:di + 0x1FFE], ax
    loop .loop
    pop es
    ret


_init:
    push cs
    pop ds

    pop es
    pop ax
    mov [_boot_arch], ax
    push es
    cmp al, ARCH_NEC98
    jz _init_n98
    cmp al, ARCH_FMT
    jz _init_fmt

    int 0x12
    mov cl, 6
    shl ax, cl
    mov [_memsz_lo], ax

    mov ah, 0x88
    stc
    int 0x15
    jc _next
    mov [_memsz_mi], ax
    jmp _next

_init_n98:

    mov al, [0x0501]
    and ax, byte 0x07
    inc ax
    mov cl, 13
    shl ax, cl
    mov [_memsz_lo], ax

    mov al, [0x0401]
    xor ah, ah
    mov cl, 7
    shl ax, cl
    mov [_memsz_mi] ,ax

    mov ax, [0x0594]
    shl ax, 4 ; shl eax, 20 -16
    mov [_memsz_hi + 2], ax

    jmp _next

_init_fmt:
    mov ax, 0xC000 ; TOWNS always > 768KB
    mov [_memsz_lo], ax

    mov dx, 0x05E8
    in al, dx
    dec al
    shl ax, 10
    mov [_memsz_mi], ax
    ;jmp _next

_next:

    ;; check cpu
    mov dx, 0xF000
    pushf
    pop ax
    mov cx, ax
    and ax, 0x0FFF
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
    or cx, dx
    push cx
    popf
    pushf
    pop ax
    and ax, dx
    jz short .bad_cpu


    ;; memory check (temp)
    cmp word [_memsz_mi], 3000
    ja .mem_ok
    mov si, no_mem_mes
    call _puts
    jmp forever
.mem_ok:

    ;; kernel signature check
    cmp word [es:_END - _HEAD], CEEF_MAGIC
    jz .elf_ok
    mov si, bad_kernel_mes
    call _puts
    jmp forever
.elf_ok:


_set_video_mode:
    mov al, [_boot_arch]
    cmp al, ARCH_NEC98
    jz _vga_nec98
    cmp al, ARCH_FMT
    jz _vga_fmt
    jmp _vesa

_bad_vga:
    mov si, vga_err_mes
    call _puts
    jmp forever

_vga_nec98:
    mov al, [0x045C]
    test al, 0x40
    jz _bad_vga

    xor al, al
    out 0xF2, al

    mov eax, 640 + 480 * 0x10000
    mov [_screen_width], eax
    mov [_screen_stride], ax
    mov dword [_vram_base], 0xFFF00000

    mov ax, 0x300C
    mov bh, 0x32
    int 0x18
    mov ah, 0x4D
    mov ch, 0x01
    int 0x18

    push ds
    push word 0xE000
    pop ds
    mov ax, 1
    mov [0x0100], al
    mov [0x0102], ax
    pop ds

    mov ah, 0x0C
    int 0x18
    mov ah, 0x40
    int 0x18

    xor cx, cx
    mov bx, MAX_PALETTE
    mov si, _palette
.loop:
    mov al,cl
    out 0xA8, al
    lodsd
    out 0xAE, al
    shr eax, 8
    out 0xAA, al
    shr eax, 8
    out 0xAC, al
    inc cx
    dec bx
    jnz .loop

    jmp _video_next

_vga_fmt:
    mov dword [_screen_width], 640 + 480 * 0x10000
    mov word [_screen_stride], 640 ;1024
    mov dword [_vram_base], 0x80100000

    xor cl, cl
    lea si, _fmt_vga_param
    mov dx, 0x0440
    mov al, 0x00
    out dx, al
    lodsw
    inc dx
    inc dx
    out dx, ax
    dec dx
    dec dx
    mov al, 0x01
    out dx, al
    lodsw
    inc dx
    inc dx
    out dx, ax

    mov cl, 0x04
.mode_loop:
    mov dx, 0x0440
    mov al, cl
    out dx, al
    lodsw
    inc dx
    inc dx
    out dx, ax
    inc cx
    cmp cl, 0x1F
    jbe .mode_loop

    mov dl, 0x48
    xor al, al
    out dx, al
    mov dl, 0x4A
    mov al, 0x0A
    out dx, al
    mov dl, 0x48
    mov al, 0x01
    out dx, al
    mov dl, 0x4A
    mov al, 0x38
    out dx, al

    mov dx, 0xFDA0
    mov al, 0x08
    out dx, al

    mov dx, 0x0440
    mov al, 0x1C
    out dx, al
    inc dx
    inc dx
    mov ax, 0x800F
    out dx, ax

    xor cx, cx
    mov bx, MAX_PALETTE
    mov si, _palette

    mov dx, 0xFD90
.loop1:
    mov al, cl
    out dx, al
    lodsd
    add dl, 2
    out dx, al
    shr eax, 8
    add dl, 4
    out dx, al
    shr eax, 8
    sub dl, 2
    out dx, al
    sub dl, 4
    inc cx
    dec bx
    jnz .loop1

    jmp _video_next

_vesa:
    sub sp, 256
    push ss
    pop es

    mov ax, 0x4F02
    mov bx, VESA_MODE
    int 0x10
    jc .no_vesa
    or ah, ah
    jnz .no_vesa
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
    mov ax, 0x0013
    int 0x10
    mov ax, 0x0F00
    int 0x10
    cmp al, 0x13
    jnz _bad_vga
    mov eax, 320 + 200 * 0x10000
    mov [_screen_width], eax
    mov [_screen_stride], ax
    mov dword [_vram_base], 0x000A0000

.next:

    xor cx, cx
    mov bx, MAX_PALETTE
    mov si, _palette

    mov dx, 0x03DA
    in al, dx
    push cx
    xor cl, cl
    mov dl, 0xC0
.loop0:
    mov al, cl
    out dx, al
    inc cx
    out dx, al
    cmp cl, 16
    jb .loop0
    pop cx
    mov dl, 0xDA
    in al, dx
    mov dl, 0xC0
    mov al, 0x20
    out dx, al
    mov dl, 0xC8
    mov al, cl
    out dx, al
    inc dl
.loop1:
    lodsd
    rol eax, 16
    shr al, 2
    out dx, al
    rol eax, 8
    shr al, 2
    out dx, al
    rol eax, 8
    shr al, 2
    out dx, al
    inc cx
    dec bx
    jnz .loop1

    add sp, 256

_video_next:
    pop cx
    cli

    lgdt [_GDT]

    mov eax, cr0
    or eax, byte 1
    mov cr0, eax
    db 0xEB, 0x00 ; just in case

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

    add edi, 0x00000FFF
    and edi, 0xFFFFF000
    lea esp, [edi + STACK_SIZE]
    mov [_kernel_end], esp

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

    push byte 0
    popfd
    mov ecx, _boot_info
    push ecx
    call [ebp + CEEF_ENTRY]
    ud2



    db 22, 0, 15
cpu_err_mes:
    db "UNSUPPORTED CPU", 13, 10, 0

    db 22, 0, 17
vga_err_mes:
    db "UNSUPPORTED VIDEO", 13, 10, 0

    db 22, 0, 17
no_mem_mes:
    db "NOT ENOUGH MEMORY", 13, 10, 0

    db 22, 0, 16
bad_kernel_mes:
    db "BAD KERNEL MAGIC", 13, 10, 0


_boot_info:
_boot_arch      db 0
_boot_drive     db 0
                dw 0
_memsz_lo       dw 0
_memsz_mi       dw 0
_memsz_hi       dd 0
_kernel_end     dd 0
_vram_base      dd 0
_screen_width   dw 0
_screen_height  dw 0
_screen_stride  dw 0
_screen_bpp     db 0
                db 0
_acpi           dd 0

    ;; FM TOWNS 640x480x8 mode parameters
_fmt_vga_param:
    dw 0x0060, 0x02C0,                 0x031F, 0x0000, 0x0004, 0x0000
    dw 0x0419, 0x008A, 0x030A, 0x008A, 0x030A, 0x0046, 0x0406, 0x0046
    dw 0x0406, 0x0000, 0x008A, 0x0000, 0x0050, 0x0000, 0x008A, 0x0000
    dw 0x0050, 0x0058, 0x0001, 0x0000, 0x000F, 0x0002, 0x0000, 0x0192

    ;;　GDT
    alignb 16
_GDT:
    dw (_end_GDT - _GDT - 1), _GDT, 0x0000, 0x0000 ;; 00 NULL
    dw 0, 0, 0, 0   ;; 08 RESERVED
    dw 0xFFFF, 0x0000, 0x9A00, 0x00CF   ;; 10 32bit KERNEL TEXT FLAT
    dw 0xFFFF, 0x0000, 0x9200, 0x00CF   ;; 18 32bit KERNEL DATA FLAT
_end_GDT:

_palette:
    dd 0xFF212121, 0xFF0D47A1, 0xFF1B5E20, 0xFF006064, 0xFFb71c1c, 0xFF4A148C, 0xFF795548, 0xFF9E9E9E,
    dd 0xFF616161, 0xFF2196F3, 0xFF4CAF50, 0xFF00BCD4, 0xFFf44336, 0xFF9C27B0, 0xFFFFEB3B, 0xFFFFFFFF,
    dd 0xFF000000, 0xFF330000, 0xFF660000, 0xFF990000, 0xFFCC0000, 0xFFFF0000, 0xFF003300, 0xFF333300,
    dd 0xFF663300, 0xFF993300, 0xFFCC3300, 0xFFFF3300, 0xFF006600, 0xFF336600, 0xFF666600, 0xFF996600,
    dd 0xFFCC6600, 0xFFFF6600, 0xFF009900, 0xFF339900, 0xFF669900, 0xFF999900, 0xFFCC9900, 0xFFFF9900,
    dd 0xFF00CC00, 0xFF33CC00, 0xFF66CC00, 0xFF99CC00, 0xFFCCCC00, 0xFFFFCC00, 0xFF00FF00, 0xFF33FF00,
    dd 0xFF66FF00, 0xFF99FF00, 0xFFCCFF00, 0xFFFFFF00, 0xFF000033, 0xFF330033, 0xFF660033, 0xFF990033,
    dd 0xFFCC0033, 0xFFFF0033, 0xFF003333, 0xFF333333, 0xFF663333, 0xFF993333, 0xFFCC3333, 0xFFFF3333,
    dd 0xFF006633, 0xFF336633, 0xFF666633, 0xFF996633, 0xFFCC6633, 0xFFFF6633, 0xFF009933, 0xFF339933,
    dd 0xFF669933, 0xFF999933, 0xFFCC9933, 0xFFFF9933, 0xFF00CC33, 0xFF33CC33, 0xFF66CC33, 0xFF99CC33,
    dd 0xFFCCCC33, 0xFFFFCC33, 0xFF00FF33, 0xFF33FF33, 0xFF66FF33, 0xFF99FF33, 0xFFCCFF33, 0xFFFFFF33,
    dd 0xFF000066, 0xFF330066, 0xFF660066, 0xFF990066, 0xFFCC0066, 0xFFFF0066, 0xFF003366, 0xFF333366,
    dd 0xFF663366, 0xFF993366, 0xFFCC3366, 0xFFFF3366, 0xFF006666, 0xFF336666, 0xFF666666, 0xFF996666,
    dd 0xFFCC6666, 0xFFFF6666, 0xFF009966, 0xFF339966, 0xFF669966, 0xFF999966, 0xFFCC9966, 0xFFFF9966,
    dd 0xFF00CC66, 0xFF33CC66, 0xFF66CC66, 0xFF99CC66, 0xFFCCCC66, 0xFFFFCC66, 0xFF00FF66, 0xFF33FF66,
    dd 0xFF66FF66, 0xFF99FF66, 0xFFCCFF66, 0xFFFFFF66, 0xFF000099, 0xFF330099, 0xFF660099, 0xFF990099,
    dd 0xFFCC0099, 0xFFFF0099, 0xFF003399, 0xFF333399, 0xFF663399, 0xFF993399, 0xFFCC3399, 0xFFFF3399,
    dd 0xFF006699, 0xFF336699, 0xFF666699, 0xFF996699, 0xFFCC6699, 0xFFFF6699, 0xFF009999, 0xFF339999,
    dd 0xFF669999, 0xFF999999, 0xFFCC9999, 0xFFFF9999, 0xFF00CC99, 0xFF33CC99, 0xFF66CC99, 0xFF99CC99,
    dd 0xFFCCCC99, 0xFFFFCC99, 0xFF00FF99, 0xFF33FF99, 0xFF66FF99, 0xFF99FF99, 0xFFCCFF99, 0xFFFFFF99,
    dd 0xFF0000CC, 0xFF3300CC, 0xFF6600CC, 0xFF9900CC, 0xFFCC00CC, 0xFFFF00CC, 0xFF0033CC, 0xFF3333CC,
    dd 0xFF6633CC, 0xFF9933CC, 0xFFCC33CC, 0xFFFF33CC, 0xFF0066CC, 0xFF3366CC, 0xFF6666CC, 0xFF9966CC,
    dd 0xFFCC66CC, 0xFFFF66CC, 0xFF0099CC, 0xFF3399CC, 0xFF6699CC, 0xFF9999CC, 0xFFCC99CC, 0xFFFF99CC,
    dd 0xFF00CCCC, 0xFF33CCCC, 0xFF66CCCC, 0xFF99CCCC, 0xFFCCCCCC, 0xFFFFCCCC, 0xFF00FFCC, 0xFF33FFCC,
    dd 0xFF66FFCC, 0xFF99FFCC, 0xFFCCFFCC, 0xFFFFFFCC, 0xFF0000FF, 0xFF3300FF, 0xFF6600FF, 0xFF9900FF,
    dd 0xFFCC00FF, 0xFFFF00FF, 0xFF0033FF, 0xFF3333FF, 0xFF6633FF, 0xFF9933FF, 0xFFCC33FF, 0xFFFF33FF,
    dd 0xFF0066FF, 0xFF3366FF, 0xFF6666FF, 0xFF9966FF, 0xFFCC66FF, 0xFFFF66FF, 0xFF0099FF, 0xFF3399FF,
    dd 0xFF6699FF, 0xFF9999FF, 0xFFCC99FF, 0xFFFF99FF, 0xFF00CCFF, 0xFF33CCFF, 0xFF66CCFF, 0xFF99CCFF,
    dd 0xFFCCCCFF, 0xFFFFCCFF, 0xFF00FFFF, 0xFF33FFFF, 0xFF66FFFF, 0xFF99FFFF, 0xFFCCFFFF, 0xFFFFFFFF,
    dd 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,

_END:
