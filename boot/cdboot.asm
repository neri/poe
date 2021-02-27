;; El Torito Boot Sector for MEG-OS
;; Licenst: MIT (c) 2009, 2021 MEG-OS project

%define IPL_SIGN    0x1eaf
%define ARCH_NEC98  0
%define ARCH_PC     1
%define ARCH_FMT    2

%define NOBOOTDIR


[bits 16]
[org 0x0800]

    ;; setup register
    cld
    xor ax, ax
    mov ds, ax
    mov es, ax

    ;; move 7C00 to 0800
    mov cx, 0x0800
    mov si, 0x7C00
    mov di, cx
    rep movsb
    push ds
    push word _next
    retf
_next:
    mov [exit_sssp], sp
    mov [exit_sssp + 2], ss

    mov ss, ax
    mov sp, 0x0800

    ;; save drive number
    mov [drive_number], dl

    push word 0x0800
    pop es

    ;; read dir
    mov eax, 16
    mov ecx, 0x800
    call _read
    mov eax, [0x809E]
    mov ecx, [0x80A6]
    mov [dir_size], ecx
    call _read

%ifndef NOBOOTDIR
    mov bp, sysdir
    call _find_file
    cmp bx, byte -1
    jz .nodir
    mov eax, [es:bx + 0x02]
    mov ecx, [es:bx + 0x0A]
    mov [dir_size], ecx
    call _read
%endif

    mov bp, sysname
    call _find_file
    cmp bx, byte -1
    jz .nofile
    mov eax, [es:bx + 0x02]
    mov ecx, [es:bx + 0x0A]
    push word 0x1000
    pop es
    call _read

    call _clear_keystroke

    mov cx, [arch_id]
    mov ax, IPL_SIGN
    push es
    push ds
    retf

.nodir:
.nofile:
    mov si, no_file_msg
    call _puts
    jmp _forever

_find_file:
    xor bx, bx
.loop:
    cmp bx, [dir_size]
    jae short .enddir
    mov al, [es:bx]
    or al, al
    jz .nofile_noentry
    cmp al, 0x20
    jbe short .enddir
    mov si, bp
    lodsb
    cmp al, byte [es:bx+0x20]
    jnz short .nofile
    movzx cx, al
    lea di, [bx+0x21]
    rep cmpsb
    jnz .nofile
    ret
.nofile:
    movzx ax, byte [es:bx]
    add bx, ax
    jmp short .loop
.nofile_noentry:
    add bx, 0x0800
    and bx, 0xF800
    jmp short .loop
.enddir:
    or bx, byte -1
    ret


_clear_keystroke:
    mov ah, 0x01
    int 0x16
    jz short .skip
    xor ax, ax
    int 0x16
    jmp short _clear_keystroke
.skip:
    ret


_read:
    push si
    mov si, lba_packet
    mov [si+0x06], es
    mov [si+0x08], eax
    add ecx, 0x7FF
    shr ecx, 11

.loop:
    push cx

    ;; display progress
%if 1
    mov ax, 0x0E2E
    int 0x10
%endif

    mov dl, [drive_number]
    mov ah, 0x42
    int 0x13

    pop cx
    jc .readerror
    add word [si+0x06], 0x0080
    inc dword [si+0x08]
    loop .loop
    pop si
    ret

.readerror:
    mov si, disk_error_msg
    call _puts
_forever:
    call _clear_keystroke
    mov si, halt_msg
    call _puts
    xor ax, ax
    int 0x16
    lss sp, [cs:exit_sssp]
    retf

_puts:
    push si
.loop:
    lodsb
    or al, al
    jz .end
    mov ah, 0x0E
    int 0x10
    jmp .loop
.end:
    pop si
    ret


; _disp_hex16:
;     mov cx, 4
;     jmp short _disphex
; _disp_hex8:
;     mov cx, 2
;     shl ax, 8
; _disphex:
;     push bx
;     call .skip
;     db "0123456789abcdef"
; .skip:
;     pop bx
;     mov dx, ax

; .loop:
;     push cx
;     rol dx, 4
;     mov al, dl
;     and al, 0x0F
;     xlat
;     mov ah, 0x0E
;     int 0x10
;     pop cx
;     loop .loop

;     mov ax, 0x0E20
;     int 0x10
;     pop bx
;     ret


no_file_msg:
    db "Missing loader.", 13, 10, 0
disk_error_msg:
    db "_read error.", 13, 10, 0
halt_msg:
    db "  [Press any key...]", 13, 10, 0

%ifndef NOBOOTDIR
sysdir:
    db 4, "BOOT"
%endif
sysname: ; basename + dot + ext + revision
    db 6+1+3+2, "KERNEL.SYS;1"

    alignb 4
lba_packet:
    db 0x10, 0
    dw 1
    dd 0
    dd 0, 0

exit_sssp       dd 0
dir_size        dd 0
arch_id         db ARCH_PC
drive_number    db 0x00
hdboot          db 0x00

    times 007FEh-($-$$) db 0
    db 055h, 0AAh
