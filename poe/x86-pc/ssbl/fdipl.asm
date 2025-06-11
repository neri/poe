;; Floppy Boot Sector for MEG-OS
;; PUBLIC DOMAIN
;;
;; # MEMORY MAP
;;
;; 0000_0000 ----------------
;;           | IVT          |
;; 0000_0400 ----------------
;;           | BDA          |
;;           - - - - - - - -
;;           | STACK        |
;; 0000_0700 ----------------
;;           | UNUSED       |
;; 0000_7C00 ----------------
;;           | THIS PROGRAM |
;; 0000_8000 ----------------
;;           | FAT BUFFER   |
;;           - - - - - - - - 
;;           | UNUSED       |
;; 0001_0000 ----------------
;;           | LOADED IMAGE |
;;           - - - - - - - - 
;;           | UNUSED       |
;; 000A_0000 ----------------
;;           | VRAM & BIOS  |
;; 000F_FFFF ----------------
;;
;; # HANDOVER
;;
;; * REAL MODE
;; * CS:IP = 0x1000:0x0000
;; * AX = signature (0x1eaf)
;; * DL = drive (ex. 0x00)
;;

%define IPL_SIGN    0x1eaf
%define ARCH_NEC98  0
%define ARCH_PC     1
%define ARCH_FMT    2

%define OFFSET_FAT  0x0400

[CPU 8086]
[BITS 16]

_HEAD:
    jmp short main
    nop
    db "IPL4MEGF"

    ;; BPB for 2HD 1440KB
bytes_per_sector    dw 0x0200
sectors_per_cluster db 1
reserved_sectors    dw 1
n_fats              db 2
n_root_entries      dw 0x00e0
total_sectors       dw 2880
media_descriptor    db 0xf0
sectors_per_fat     dw 9
sectors_per_track   dw 18
n_heads             dw 2
hidden_sectors      dw 0

;;  Runtime Variables
fat2        dw 0    ; 2
arch_id     db 0    ; 1
drive       db 0    ; 1
param_n     db 0    ; 1
clust_sft   db 0    ; 1

    times 0x26 - ($-$$) db 0

    db 0x29
    dd 0xffffffff
    ;;  123456789AB
    db "NO NAME    "
    db "FAT12   "


main:
    xor si, si
    push si
    popf
    mov ss, si
    mov sp, 0x0700

    ;; select architecture
    mov di, arch_id
    mov ax, cs
    mov cx, 0x07c0
    push cx
    cmp ah, 0x1f
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
    mov al, [bytes_per_sector+1]
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

    mov al, [sectors_per_cluster]
    xor dx, dx
.loop_clst_sft:
    shr al, 1
    jz .end
    inc dx
    jmp .loop_clst_sft
.end:
    mov [clust_sft], dl

    ;; read Root DIR
    mov ax, [n_root_entries]
    mov cl, 5
    shl ax, cl
    mov cx, ax
    xor dx, dx
    div word [bytes_per_sector]
    mov si, [sectors_per_fat]
    add si, si
    add si, [reserved_sectors]
    add si, [hidden_sectors]
    add ax, si
    mov [fat2], ax
    xor bp, bp
    call diskread

    ;; Read FAT
    mov ax, [sectors_per_fat]
    mul word [bytes_per_sector]
    xchg ax, cx
    mov si, 1
    push cs
    pop es
    mov bp, OFFSET_FAT
    call diskread
    pop es

    ;; Find System
    mov cx, [n_root_entries]
    xor di, di
.loop_find:
    push cx
    mov si, sysname
    mov cx, 11
    rep cmpsb
    pop cx
    jz .found
    or di, byte 0x1f
    inc di
    loop .loop_find
    jmp forever
.found:
    and di, byte 0xe0
    mov bx, [bytes_per_sector]
    mov si, [es:di+0x001A]
    xor bp, bp
    push es
    push bp
.loop_sector:
    cmp si, 0x0ff7
    jae .end_sector
    push si
    mov cl, [clust_sft]
    dec si
    dec si
    shl si, cl
    add si, [fat2]
    mov dx, [bytes_per_sector]
    shl dx, cl
    mov cx, dx
    call diskread
    pop ax
    mov bx, ax
    add bx, bx
    add bx, ax
    shr bx, 1
    mov si, [cs:bx+OFFSET_FAT]
    jnc .even
    mov cl, 4
    shr si, cl
.even:
    and si, 0x0fff
    jmp short .loop_sector
.end_sector:

    ;; jump system
    mov ax, IPL_SIGN
    mov cx, [arch_id]
_retf:
    retf


    ;; disk read
    ;; IN cx:size si:LBA es:bp:buffer
    ;; USES ax cx bx dx
diskread:
    xchg ax, cx
    xor dx, dx
    div word [bytes_per_sector]
    xchg ax, cx
.loop:
    push cx

    xor dx, dx
    mov ax, si
    div word [sectors_per_track]
    inc dx
    div byte [n_heads]
    mov dh, ah
    mov bx, [bytes_per_sector]
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
    int 0x1b
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
    call 0xfffb:0x0014
    pop di
    pop ds
    pop bx
    jmp short .after_read


forever:
    jmp short $

sysname:
    ;;  FilenameExt
    ;;  12345678123
    db "OSLDR   SYS"

    times 0x01fe - ($-$$) db 0
    db 0x55, 0xaa
