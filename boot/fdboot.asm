;; Regular Floppy Boot Sector for MEG-OS
;; WTFPL/PUBLIC DOMAIN

%define IPL_SIGN    0x1eaf
%define ARCH_PC     1
%define ARCH_NEC98  0
%define ARCH_FMT    2

[CPU 8086]
[BITS 16]

_HEAD:
    jmp short main
    nop
    db "IPL4MEGF"

    ;; BPB for 2HD 1440KB
    dw 0x0200
    db 1
    dw 1
    db 2
    dw 0x00E0
    dw 2880
    db 0xF0
    dw 9
    dw 18
    dw 2

;;  Variables
fat2        dw 0    ; 2
arch_id     db 0    ; 1
drive       db 0    ; 1
param_n     db 0    ; 1
clust_sft   db 0    ; 1

    times 0x26 - ($-$$) db 0

    db 0x29
    dd 0xFFFFFFFF
    ;;  123456789AB
    db "NO NAME    "
    db "FAT12   "

main:

    ;; setup register
    xor si, si
    push si
    popf
    mov ss, si
    mov sp, 0x0700

    ;; select architecture
    mov di, arch_id
    mov ax, cs
    mov cx, 0x07C0
    push cx
    cmp ah, 0x1F
    ja short initFMT
    jz short init98

    ;; IBM PC COMPATIBLE
initAT:
    mov ax,(main0-_HEAD)
    push ax
    retf
    
main0:
    push cs
    pop ds
    inc byte [di]
    xor ax, ax
    int 0x13
    jmp short _next

    ;; FM TOWNS
initFMT:
    push cs
    pop ds
    mov ax, 0x2002
    or  ah, bh
    mov [di], ax
    jmp short init2

    ;; NEC PC-98
init98:
    push cs
    pop ds
    mov al, [ss:0x0584]
    mov [drive], al
init2:
    mov al, [0x000C]
    shr al, 1
    inc al
    mov [param_n], al
    pop es
    xor di, di
    mov cx, 256
    rep movsw
    push es
    call _retf
    push cs
    pop ds

_next:
    mov ax, 0x1000
    mov es, ax
    push es

    mov al, [0x000D]
    xor dx, dx
.loop_clst_sft:
    shr al, 1
    jz .end
    inc dx
    jmp .loop_clst_sft
.end:
    mov [clust_sft], dl

    ;; read Root DIR
    mov ax, [0x0011]
    mov cl, 5
    shl ax, cl
    mov cx, ax
    mov si, [0x0016]
    add si, si
    inc si
    xor dx, dx
    div word [0x000B]
    add ax, si
    mov [fat2], ax
    xor bp, bp
    call diskread

    ;; Read FAT
    mov ax, [0x0016]
    mul word [0x000B]
    xchg ax, cx
    mov si, 1
    push cs
    pop es
    mov bp, 0x0400
    call diskread
    pop es

    ;; Find System
    mov cx, [0x0011]
    xor di, di
.loop_find:
    push cx
    mov si, sysname
    mov cx, 11
    rep cmpsb
    pop cx
    jz .found
    or di, byte 0x1F
    inc di
    loop .loop_find
    jmp forever
.found:
    and di, byte 0xE0
    mov bx, [0x000B]
    mov si, [es:di+0x001A]
    xor bp, bp
    push es
    push bp
.loop_sector:
    cmp si, 0x0FF7
    jae .end_sector
    push si
    mov cl, [clust_sft]
    dec si
    dec si
    shl si, cl
    add si, [fat2]
    mov dx, [0x000B]
    shl dx, cl
    mov cx, dx
    call diskread
    pop ax
    mov bx, ax
    add bx, bx
    add bx, ax
    shr bx, 1
    mov si, [cs:bx+0x0400]
    jnc .even
    mov cl, 4
    shr si, cl
.even:
    and si, 0x0FFF
    jmp short .loop_sector
.end_sector:

    ;; jump system
    mov ax, IPL_SIGN
    mov cx, [arch_id]
_retf:
    retf


    ;; disk read
diskread:
    xchg ax, cx
    xor dx, dx
    div word [0x000B]
    xchg ax, cx
.loop:
    push cx

    xor dx, dx
    mov ax, si
    div word [0x0018]
    inc dx
    shr ax, 1
    adc dh, 0
    mov bx, [0x000B]
    cmp byte [arch_id], ARCH_NEC98
    jz short .nec98
    cmp byte [arch_id], ARCH_FMT
    jz short .fmt
    mov ch, al
    mov cl, dl
    mov dl, [drive]
    xchg bx, bp
    mov ax, 0x0201
    int 0x13
    xchg bx, bp
    jmp short .after_read
.nec98:
    mov cl, al
    mov ch, [param_n]
    mov al, [drive]
    mov ah, 0x56
    int 0x1B
.after_read:
    jnc .next
    jmp forever
.next:
    mov cl, 4
    shr bx, cl
    mov ax, es
    add ax, bx
    mov es, ax
    inc si
    pop cx
    loop .loop
    ret

.fmt:
    push bx
    push ds
    push di
    mov cl, al
    mov al, [drive]
    mov ah, 0x05
    mov bx, 0x0001
    push es
    pop ds
    mov di, bp
    call 0xFFFB:0x0014
    pop di
    pop ds
    pop bx
    jmp short .after_read


forever:
    jmp short $

sysname:
    ;;  FILENAMEEXT
    db "KERNEL  SYS"

    times 0x01FE - ($-$$) db 0
    db 0x55, 0xAA
