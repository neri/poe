# Architecture of MEG-OS Lite

## Kernel

- TBD


## Boot Sequence (PC)

``` plantuml
@startuml
title BIOS to Kernel
start

partition BIOS {
    :Some initialization processes;
}

partition IPL {
    :load KERNEL.SYS from BootDisk;
    :invoke Loader at the top of the KERNEL.SYS;
}

partition Loader_(Real_Mode) {
    :relocate Loader to ORG_BASE (0000:0800);
    if (check CPU, Memory and Video mode) then (OK)
    else (NG)
        :error;
        end
    endif
    :set Video mode;
    :clear A20 mask;
    :enter to Protected Mode;
}
partition Loader_(Protected_Mode) {
    :relocate the kernel from KERNEL.SYS;
    :invoke Kernel;
}
:Kernel entry point;
stop

@enduml
```

``` plantuml
@startuml
title Initialization of Kernel
start
:entry point;
:System::init();
partition System::init() {
    :make main_screen;
    :MemoryManager::init();
    :Arch::init();
    :Scheduler::start();
}
split 
    :idle;
    detach
split again
    :System::late_init();
    partition System::late_init() {
        :WindowManager::init();
        :HidManager::init();
        :Arch::late_init();
    }
    :Shell::main();
    stop
end split

@enduml
```

## Memory Manager

- MEG-OS allocates large memory blocks in pages. Smaller memory blocks are allocated with a slab allocator.
- MEG-OS Lite does not support any features of the MMU to protect the system.

## Scheduler

- MEG-OS supports five priority-based preemptive multi-threaded schedulers.
- Priority **Real-time** is scheduled with the highest priority and is never preempted.
- The **high**, **normal**, and **low** priorities are each scheduled in a round-robin fashion and will be preempted when the allocated Quantum is consumed.
- Priority **idle** makes the processor idle when all other threads are waiting. It is never scheduled.

## Window System

- TBD

## Hid Manager

- HidManager relays between human interface devices and the window event subsystem
- Keyboard scancodes will be converted to the Usage specified by the USB-HID specification on all platforms

## FileSystem

- TBD

## User Land (Personality)

- TBD
