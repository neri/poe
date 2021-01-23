;; TOE ASM PART

[bits 32]
[section .text]

    global memset
memset:
    push edi
    mov edi, [esp+8]
    mov al, [esp+12]
    mov ecx, [esp+16]
    mov edx, edi
    rep stosb
    mov eax, edx
    pop edi
    ret

    ; pub unsafe extern "C" fn cpu_default_exception(ctx: &mut StackContext)
    extern cpu_default_exception
    ; pub unsafe extern "fastcall" fn apic_handle_irq(irq: Irq)
    extern pic_handle_irq

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

_asm_int_0D: ; #GP General Protection Fault
    push BYTE 0x0D
    jmp short _exception

_asm_int_0E: ; #PF Page Fault
    push BYTE 0x0E
    ; jmp short _exception

_exception:
    push es
    push ss
    push ds
    pushad
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


_irq0:
    push ecx
    mov cl, 0
    jmp short _irq

_irq1:
    push ecx
    mov cl, 1
    jmp short _irq

_irq2:
    push ecx
    mov cl, 2
    jmp short _irq

_irq3:
    push ecx
    mov cl, 3
    jmp short _irq

_irq4:
    push ecx
    mov cl, 4
    jmp short _irq

_irq5:
    push ecx
    mov cl, 5
    jmp short _irq

_irq6:
    push ecx
    mov cl, 6
    jmp short _irq

_irq7:
    push ecx
    mov cl, 7
    jmp short _irq

_irq8:
    push ecx
    mov cl, 8
    jmp short _irq

_irq9:
    push ecx
    mov cl, 9
    jmp short _irq

_irq10:
    push ecx
    mov cl, 10
    jmp short _irq

_irq11:
    push ecx
    mov cl, 11
    jmp short _irq

_irq12:
    push ecx
    mov cl, 12
    jmp short _irq

_irq13:
    push ecx
    mov cl, 13
    jmp short _irq

_irq14:
    push ecx
    mov cl, 14
    jmp short _irq

_irq15:
    push ecx
    mov cl, 15

_irq:
    push eax
    push edx
    push ds
    push es
    cld

    call pic_handle_irq

    pop es
    pop ds
    pop edx
    pop eax
    pop ecx
    iretd


    global asm_handle_irq_table
asm_handle_irq_table:
    push esi
    push edi
    mov esi, _irq_table
    mov edi, ecx
    mov ecx, 16
    rep movsd
    pop edi
    pop esi
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

_irq_table:
    dd _irq0
    dd _irq1
    dd _irq2
    dd _irq3
    dd _irq4
    dd _irq5
    dd _irq6
    dd _irq7
    dd _irq8
    dd _irq9
    dd _irq10
    dd _irq11
    dd _irq12
    dd _irq13
    dd _irq14
    dd _irq15
