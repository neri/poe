;; MEG-OS Loader for TOE
;; License: MIT (c) 2021 MEG-OS project

%define IPL_SIGN            0x1eaf
%define PF_NEC98            1   ; NEC PC-98 Series Computer
%define PF_PC               2   ; IBM PC/AT Compatible
%define PF_FMT              3   ; Fujitsu FM TOWNS

%define ORG_BASE            0x0800

%define KERNEL_CS           0x10
%define KERNEL_DS           0x18
%define STACK_SIZE          0x1000

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

%define VESA_MODE_1         0x4112 ; 640x480x32
%define VESA_MODE_2         0x4101 ; 640x480x8
%define MAX_PALETTE         256

%define SMAP_AVAILABLE      0x01
%define SMAP_RESERVED       0x02
%define SMAP_ACPI_RECLAIM   0x03
%define SMAP_ACPI_NVS       0x04

[BITS 16]
[ORG ORG_BASE]

_HEAD:
    jmp short _crt0
    dw _END - _HEAD
    db "TOE" ,0

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
    push ds

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
    inc ax
    mov [_platform], ax
    push es

    ;; check cpu
_check_cpu:
    ;; is 286 or later
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
    ;; is 386 or later
    or cx, dx
    push cx
    popf
    pushf
    pop ax
    and ax, dx
    jz short .bad_cpu
    mov di, 3

    ;; is 486 or later
    pushfd
    pop eax
    mov ecx, eax
    xor eax, 0x00040000 ; AC
    push eax
    popfd
    pushfd
    pop eax
    cmp eax, ecx
    jz .end_cpu
    inc di

    ; has cpuid?
    mov eax, ecx
    xor eax, 0x00200000 ; ID
    push eax
    popfd
    pushfd
    pop eax
    xor eax, ecx
    jz .end_cpu
    inc di

.end_cpu:
    mov ax, di
    mov [_cpu_ver], al

_mem_check:
    mov al, [_platform]
    cmp al, PF_PC
    jz _memchk_pc
    cmp al, PF_FMT
    jz _memchk_fmt

_memchk_n98:
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

    ; mov ax, [0x0594]
    ; shl ax, 4 ; shl eax, 20 -16
    ; mov [_memsz_hi + 2], ax
    jmp _end_mem_check

_memchk_fmt:
    mov ax, 0xC000 ; TOWNS always > 768KB
    mov [_memsz_lo], ax

    mov dx, 0x05E8
    in al, dx
    dec al
    and eax, 0x7F
    shl eax, 20
    mov [_memsz_mid], eax
    jmp _end_mem_check

_memchk_pc:
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
    mov eax, 0xE820
    mov edx, 0x534D4150 ; SMAP
    mov ecx, 20
    int 0x15
    cmp eax, 0x534D4150 ; SMAP
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

    jmp _end_mem_check


_end_mem_check:

    ;; memory check (temp)
    cmp word [_memsz_mid + 2], 0x0030
    jae .mem_ok
    mov si, no_mem_mes
    call _puts
    jmp forever
.mem_ok:

    ;; kernel signature check
    cmp word [es:_END - _HEAD], CEEF_MAGIC
    jz .kernel_ok
    mov si, bad_kernel_mes
    call _puts
    jmp forever
.kernel_ok:


_find_rsdptr:
    mov dx, 0xE000
.loop:
    mov es, dx
    mov si, _RSDPtr
    xor di, di
    mov cx, 4
    rep cmpsw
    jz .found
    inc dx
    jnz .loop
.not_found:
    jmp short _no_acpi

.found:
    mov bx, es
    movzx ebx, bx
    shl ebx, 4
    mov [_acpi_rsdptr], ebx
_no_acpi:


_set_video_mode:
    mov al, [_platform]
    cmp al, PF_NEC98
    jz _vga_nec98
    cmp al, PF_FMT
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

    ;; A20
    xor al, al
    out 0xF2, al

    mov eax, 640 + 480 * 0x10000
    mov [_screen_width], eax
    mov [_screen_stride], ax
    ;; TODO: ISA hole check
    mov dword [_vram_base], 0x00F00000

    mov ax, 0x300C
    mov bh, 0x32
    int 0x18
    mov ah, 0x4D
    mov ch, 0x01
    int 0x18

    push ds
    push word 0xE000
    pop ds
    xor al, al
    mov [0x0100], al ; packed pixel
    mov al, 1
    mov [0x0102], al ; linear frame buffer
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

    ;; A20 control
    in al, 0x92
    or al, 2
    out 0x92, al

;     mov ax, 0x4F02
;     mov bx, VESA_MODE_1
;     int 0x10
;     cmp ax, 0x004F
;     jnz .vesa_next
;     mov ax, 0x4F01
;     mov cx, bx
;     mov di, sp
;     int 0x10
;     cmp ax, 0x004F
;     jnz .vesa_next
;     mov al, [es:di + 0x19]
;     cmp al, 8
;     jz .vesa_ok
;     cmp al, 32
;     jz .vesa_ok
; .vesa_next:
    mov ax, 0x4F02
    mov bx, VESA_MODE_2
    int 0x10
    cmp ax, 0x004F
    jnz .no_vesa
    mov ax, 0x4F01
    mov cx, bx
    mov di, sp
    int 0x10
    cmp ax, 0x004F
    jnz .no_vesa
    mov al, [es:di + 0x19]
    cmp al, 8
    jz .vesa_ok
    cmp al, 32
    jnz .no_vesa

.vesa_ok:
    mov [_screen_bpp], al
    movzx cx, al
    shr cx, 3
    mov eax, [es:di + 0x12]
    mov [_screen_width], eax
    mov ax, [es:di + 0x10]
    xor dx, dx
    div cx
    mov [_screen_stride], ax
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
    mov ecx, esp
    mov eax, [_kernel_end]
    add eax, [_memsz_mid]
    mov [_kernel_end], ecx
    sub eax, ecx
    mov [_memsz_mid], eax

    movzx edx, byte [ebp + CEEF_N_SECS]
    lea ebx, [ebp + CEEF_OFF_SECHDR]
    mov esi, edx
    shl esi, 4
    add esi, ebx
.loop:
    mov al, [ebx]
    and al, 0xE0
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
    sub esp, 12
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

_RSDPtr:
    db "RSD PTR "


    alignb 16
_boot_info:
_platform       db 0
_boot_drive     db 0
_cpu_ver        db 0
_screen_bpp     db 8
_vram_base      dd 0
_screen_width   dw 0
_screen_height  dw 0
_screen_stride  dw 0
_boot_flags     dw 0
_acpi_rsdptr    dd 0
_initrd_base    dd 0
_initrd_size    dd 0

_smap:
_kernel_end     dd 0x00100000
_memsz_mid      dd 0

_memsz_lo       dw 0

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
    dd 0x212121, 0x0D47A1, 0x1B5E20, 0x006064, 0xb71c1c, 0x4A148C, 0x795548, 0x9E9E9E,
    dd 0x616161, 0x2196F3, 0x4CAF50, 0x00BCD4, 0xf44336, 0x9C27B0, 0xFFEB3B, 0xFFFFFF,
    dd 0x000000, 0x330000, 0x660000, 0x990000, 0xCC0000, 0xFF0000, 0x003300, 0x333300,
    dd 0x663300, 0x993300, 0xCC3300, 0xFF3300, 0x006600, 0x336600, 0x666600, 0x996600,
    dd 0xCC6600, 0xFF6600, 0x009900, 0x339900, 0x669900, 0x999900, 0xCC9900, 0xFF9900,
    dd 0x00CC00, 0x33CC00, 0x66CC00, 0x99CC00, 0xCCCC00, 0xFFCC00, 0x00FF00, 0x33FF00,
    dd 0x66FF00, 0x99FF00, 0xCCFF00, 0xFFFF00, 0x000033, 0x330033, 0x660033, 0x990033,
    dd 0xCC0033, 0xFF0033, 0x003333, 0x333333, 0x663333, 0x993333, 0xCC3333, 0xFF3333,
    dd 0x006633, 0x336633, 0x666633, 0x996633, 0xCC6633, 0xFF6633, 0x009933, 0x339933,
    dd 0x669933, 0x999933, 0xCC9933, 0xFF9933, 0x00CC33, 0x33CC33, 0x66CC33, 0x99CC33,
    dd 0xCCCC33, 0xFFCC33, 0x00FF33, 0x33FF33, 0x66FF33, 0x99FF33, 0xCCFF33, 0xFFFF33,
    dd 0x000066, 0x330066, 0x660066, 0x990066, 0xCC0066, 0xFF0066, 0x003366, 0x333366,
    dd 0x663366, 0x993366, 0xCC3366, 0xFF3366, 0x006666, 0x336666, 0x666666, 0x996666,
    dd 0xCC6666, 0xFF6666, 0x009966, 0x339966, 0x669966, 0x999966, 0xCC9966, 0xFF9966,
    dd 0x00CC66, 0x33CC66, 0x66CC66, 0x99CC66, 0xCCCC66, 0xFFCC66, 0x00FF66, 0x33FF66,
    dd 0x66FF66, 0x99FF66, 0xCCFF66, 0xFFFF66, 0x000099, 0x330099, 0x660099, 0x990099,
    dd 0xCC0099, 0xFF0099, 0x003399, 0x333399, 0x663399, 0x993399, 0xCC3399, 0xFF3399,
    dd 0x006699, 0x336699, 0x666699, 0x996699, 0xCC6699, 0xFF6699, 0x009999, 0x339999,
    dd 0x669999, 0x999999, 0xCC9999, 0xFF9999, 0x00CC99, 0x33CC99, 0x66CC99, 0x99CC99,
    dd 0xCCCC99, 0xFFCC99, 0x00FF99, 0x33FF99, 0x66FF99, 0x99FF99, 0xCCFF99, 0xFFFF99,
    dd 0x0000CC, 0x3300CC, 0x6600CC, 0x9900CC, 0xCC00CC, 0xFF00CC, 0x0033CC, 0x3333CC,
    dd 0x6633CC, 0x9933CC, 0xCC33CC, 0xFF33CC, 0x0066CC, 0x3366CC, 0x6666CC, 0x9966CC,
    dd 0xCC66CC, 0xFF66CC, 0x0099CC, 0x3399CC, 0x6699CC, 0x9999CC, 0xCC99CC, 0xFF99CC,
    dd 0x00CCCC, 0x33CCCC, 0x66CCCC, 0x99CCCC, 0xCCCCCC, 0xFFCCCC, 0x00FFCC, 0x33FFCC,
    dd 0x66FFCC, 0x99FFCC, 0xCCFFCC, 0xFFFFCC, 0x0000FF, 0x3300FF, 0x6600FF, 0x9900FF,
    dd 0xCC00FF, 0xFF00FF, 0x0033FF, 0x3333FF, 0x6633FF, 0x9933FF, 0xCC33FF, 0xFF33FF,
    dd 0x0066FF, 0x3366FF, 0x6666FF, 0x9966FF, 0xCC66FF, 0xFF66FF, 0x0099FF, 0x3399FF,
    dd 0x6699FF, 0x9999FF, 0xCC99FF, 0xFF99FF, 0x00CCFF, 0x33CCFF, 0x66CCFF, 0x99CCFF,
    dd 0xCCCCFF, 0xFFCCFF, 0x00FFFF, 0x33FFFF, 0x66FFFF, 0x99FFFF, 0xCCFFFF, 0xFFFFFF,
    dd 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,

_END:
