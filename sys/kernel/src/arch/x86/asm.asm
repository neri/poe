;; TOE ASM PART

[bits 32]
[section .text]

    ; pub unsafe extern "C" fn cpu_default_exception(ctx: &mut StackContext)
    extern cpu_default_exception

    global _asm_int_00
    global _asm_int_03
    global _asm_int_06
    global _asm_int_08
    global _asm_int_0d
    global _asm_int_0e

_asm_int_00: ; #DE Divide Error
    push BYTE 0
    push BYTE 0x00
    jmp short _exception

_asm_int_03: ; #BP Breakpoint
    push BYTE 0
    push BYTE 0x03
    jmp short _exception

_asm_int_06: ; #UD Invalid Opcode
    push BYTE 0
    push BYTE 0x06
    jmp short _exception

_asm_int_08: ; #DF Double Fault
    push BYTE 0x08
    jmp short _exception

_asm_int_0d: ; #GP General Protection Fault
    push BYTE 0x0D
    jmp short _exception

_asm_int_0e: ; #PF Page Fault
    push BYTE 0x0E
    ; jmp short _exception

_exception:
    pushad
    push es
    push ss
    push ds
    mov eax, cr2
    push eax
    mov ebp, esp
    and esp, byte 0xF0
    cld

    push ebp
    call cpu_default_exception

    mov esp, ebp
    add esp, byte 4
    pop es
    pop ds
    popad
    add esp, byte 8 ; err/intnum
_iretd:
    iretd

    global asm_handle_exception
asm_handle_exception:
    cmp cl, 15
    ja .no_exception
    movzx ecx, cl
    mov eax, [_exception_table + ecx * 4]
    ret
.no_exception:
    xor eax, eax
    ret

[section .rodata]
_exception_table:
    dd _asm_int_00
    dd 0 ; int_01
    dd 0 ; int_02
    dd _asm_int_03
    dd 0 ; int_04
    dd 0 ; int_05
    dd _asm_int_06
    dd 0 ; int_07
    dd _asm_int_08
    dd 0 ; int_09
    dd 0 ; int_0A
    dd 0 ; int_0B
    dd 0 ; int_0C
    dd _asm_int_0D
    dd _asm_int_0E
    dd 0 ; int_0F
