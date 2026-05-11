;; MEG-OS CD Boot Sector for FM TOWNS and NP21
;; License: MIT (c) 2021 MEG-OS project

%define IPL_SIGN    0x1eaf
%define ARCH_NEC98  0
%define ARCH_PC     1
%define ARCH_FMT    2

%define NOBOOTDIR

%ifndef SYSTEM_NAME
%define SYSTEM_NAME "KERNEL.SYS"
%endif
%strcat SYSNAME_REV SYSTEM_NAME, ";1"
%strlen SYSNAME_LEN SYSNAME_REV

[bits 16]
[org 0x0800]

_HEAD:

    jmp short entry
    nop

    db "IPL4"

entry:
    ;; setup register
    cld
    push cs
    pop ds
    xor ax, ax
    mov es, ax

    mov ax, cs
    cmp ah, 0x1f
    ja short initFMT
    jz short init98

_forever:
    jmp short _forever

    ;; FM TOWNS
initFMT:
    ; Select ANK font rom
    mov dx, 0xff99
    mov al, 0x01
    out dx, al

    mov ax, 0xC002
    or  ah, bh
    jmp short init2

    ;; NEC PC-98
init98:
    mov al, 0x00
    mov ah, [es:0x0584]
init2:
    xor si, si
    mov di, 0x0800
    mov cx, _END - _HEAD
    rep movsb
    push es
    push word _next
    retf

_next:
    push es
    pop ds
    mov [exit_sssp], sp
    mov [exit_sssp + 2], ss
    mov [arch_id], ax

    xor ax, ax
    mov cx, 0x0800
    mov ss, ax
    mov sp, cx
    mov es, cx

    ;; read dir
    mov eax, 16
    mov ecx, 0x0800
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
    ; jmp _exit

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
    cmp byte [arch_id], ARCH_NEC98
    jz short _read_nec98
    cmp byte [arch_id], ARCH_FMT
    jz short _read_fmt
    jmp _forever

_read_fmt:
    push ds
    push es
    pop ds

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

_read_nec98:
    push es
.loop:
    push eax
    push ecx

    mov cx, ax
    mov edx, eax
    shr edx, 16
    mov al, [cs:drive_number]
    and al, 0x7f
    mov ah, 0x06
    xor bp, bp
    mov ebx, 0x0800
    int 0x1b
    jc _readerror

    pop ecx
    pop eax
    sub ecx, ebx
    jna .done
    inc eax
    push es
    pop dx
    add dx, 0x0080
    push dx
    pop es
    jmp .loop
.done:
    pop es
    ret

_readerror:
    mov si, disk_error_msg
    call _puts
_exit:
    cmp byte [arch_id], ARCH_FMT
    jz short _puts_fmt
    jmp _forever
_exit_fmt:
    lss sp, [cs:exit_sssp]
    stc
    retf

_puts:
    cmp byte [arch_id], ARCH_NEC98
    jz short _puts_nec98
    cmp byte [arch_id], ARCH_FMT
    jz short _puts_fmt
    jmp _forever

_puts_fmt:
    push es
    pusha
    mov ax, 0xc000
    mov es, ax

    mov dx, 0xff81
    mov al, 0x07
    out dx, al

    mov di, 22 * 80 * 16
.loop:
    lodsb
    or al, al
    jz .end
    movzx bx, al
    shl bx, 3
    add bx, 0xa000

    mov cx, 8
    push di
.loop_font:
    mov al, [es:bx]
    mov [es:di], al
    mov [es:di+80], al
    add di, 160
    inc bx
    loop .loop_font
    pop di
    inc di

    jmp .loop
.end:

    popa
    pop es
    ret

_puts_nec98:
    push es
    mov ax, 0xa000
    mov es, ax
    xor di, di
    xor ah, ah
.loop:
    lodsb
    or al, al
    jz .end
    stosw
    mov al, 0xe1
    mov [es:di + 0x1ffe], ax
    jmp .loop
.end:
    pop es
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

; debug_msg:
;     db "########",0

no_file_msg:
    db "Missing system.",0

disk_error_msg:
    db "Disk read error.",0

%ifndef NOBOOTDIR
sysdir:
    db 4, "BOOT"
%endif
sysname:
    db SYSNAME_LEN, SYSNAME_REV

    alignb 4
exit_sssp       dd 0
dir_size        dd 0
arch_id         db ARCH_FMT
drive_number    db 0xC0

    times 0x3fe - ($-$$) db 0
    db 0x55, 0xaa

_END:
