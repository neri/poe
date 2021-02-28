;; MEG-OS CD Boot Sector for FM TOWNS
;; License: MIT (c) 2021 MEG-OS project

%define IPL_SIGN    0x1eaf
%define ARCH_NEC98  0
%define ARCH_PC     1
%define ARCH_FMT    2

%define NOBOOTDIR

[bits 16]
[org 0x0800]

_HEAD:

    db "IPL4"

entry:
    ;; setup register
    cld
    push cs
    pop ds
    xor ax, ax
    mov es, ax

    ;; move B000 to 0800
    xor si, si
    mov di, 0x0800
    mov cx, _END - _HEAD
    rep movsb
    push es
    push word _next
    retf
_next:
    mov ds, ax
    mov [exit_sssp], sp
    mov [exit_sssp + 2], ss
    mov cx, 0x0800
    mov ss, ax
    mov sp, cx
    mov es, cx

    mov al, bh
    or al, 0xC0
    mov [drive_number], al

    call _progress

    ;; read dir
    mov eax, 16
    mov ecx, 0x0800
    call _read
    call _progress
    mov eax, [0x809E]
    mov ecx, [0x80A6]
    mov [dir_size], ecx
    call _read
    call _progress

%ifndef NOBOOTDIR
    mov bp, sysdir
    call _find_file
    cmp bx, byte -1
    jz .nodir
    call _progress
    mov eax, [es:bx + 0x02]
    mov ecx, [es:bx + 0x0A]
    mov [dir_size], ecx
    call _read
    call _progress
%endif

    mov bp, sysname
    call _find_file
    cmp bx, byte -1
    jz .nofile
    call _progress
    mov eax, [es:bx + 0x02]
    mov ecx, [es:bx + 0x0A]
    push word 0x1000
    pop es
    call _read
    call _progress

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

_read:
    push ds
    push es
    pop ds

    ; mov al, [drive_number]
    ; mov ah, 0x03
    ; call 0xFFFB:0x0014

    mov edx, eax
    add ecx, 0x7FF
    shr ecx, 11
    mov bx, cx
    mov ecx, edx
    shr ecx, 16
    mov al, [cs:drive_number]
    mov ah, 0x05
    push es
    pop ds
    xor di, di
    call 0xFFFB:0x0014
    or ah, ah
    jnz _readerror

    pop ds
    ret

_readerror:
    mov si, disk_error_msg
    call _puts
_forever:
    hlt
    jmp _forever
    ; lss sp, [cs:exit_sssp]
    ; stc
    ; retf

_puts:
    push es
    pusha
    sub si, 3
    mov bh, 0x02
    call 0xFFFB:0x001E
    popa
    pop es
    ret

_progress:
    pushad
    mov si, tick_msg
    call _puts
    inc byte [si-2]
    popad
    ret

; _debug_hex32:
;     push ds
;     pusha
;     push cs
;     pop ds
;     xor si,si
; .loop:
;     rol edx, 4
;     mov al, dl
;     and al, 0x0F
;     mov bx, hextable
;     xlat
;     mov [debug_msg+si], al
;     inc si
;     cmp si, 8
;     jb .loop

;     mov si, debug_msg
;     call _puts
;     inc byte [debug_msg-3]
;     popa
;     pop ds
;     ret

; hextable:
;     db "0123456789ABCDEF"

;     db 0, 0, 8
; debug_msg:
;     db "########"

    db 20, 0, 1
tick_msg:
    db "."

    db 22, 0, 15
no_file_msg:
    db "Missing system."

    db 22, 0, 16
disk_error_msg:
    db "Disk read error."

%ifndef NOBOOTDIR
sysdir:
    db 4, "BOOT"
%endif
sysname: ; basename + dot + ext + revision
    db 6+1+3+2, "KERNEL.SYS;1"

    alignb 4
exit_sssp       dd 0
dir_size        dd 0
arch_id         db ARCH_FMT
drive_number    db 0xC0

    times 0x200 - ($-$$) db 0
    ; db 0x55, 0xAA

_END:
