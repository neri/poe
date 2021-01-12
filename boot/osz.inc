;; OSZ-2
;; Copyright (c) 2021 MEG-OS project

%define OSZ_SYSTBL_SIZE     0x20
%define OSZ_SYSTBL_CALLBIOS 0x00 ; INST (JMP FAR)
%define OSZ_SYSTBL_BIOS     0x01 ; FAR16PTR
%define OSZ_SYSTBL_CPUID    0x05 ; BYTE
%define OSZ_SYSTBL_ARCH     0x06 ; BYTE
%define OSZ_SYSTBL_BOOTDRV  0x07 ; BYTE
%define OSZ_SYSTBL_MEMSZ    0x08 ; WORD (IN PARA)
%define OSZ_SYSTBL_PMEMSZ   0x0A ; WORD (IN KB)
%define OSZ_SYSTBL_RAMD     0x0C ; WORD (SEG)
%define OSZ_SYSTBL_RAMDSZ   0x0E ; WORD (TEMP)

%define OSZ_ARCH_PC         1   ; IBM PC/AT Compatible
%define OSZ_ARCH_NEC98      0   ; NEC PC-98 Series Computer
%define OSZ_ARCH_FMT        2   ; Fujitsu FM TOWNS
