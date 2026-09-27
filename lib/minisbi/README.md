# Minimal SBI implementation for minios

This is a minimal SBI implementation that provides only the necessary functions for minios to run on RISC-V virt machine.

## Status

Runs on QEMU `virt` (RV32, single hart). Used by `poe/rv32-virt`.

Supported:

- Console: legacy `console_putchar` / `console_getchar`
- Timer: legacy `set_timer`, `TIME` extension
- Power: legacy `shutdown`, `SRST` extension (reboot / shutdown via syscon)
- Base extension

Not supported:

- Multiple harts (`HSM`, `IPI`, `RFENCE`)
- RV64
- Platforms other than QEMU `virt`

## License

MIT License

(c)2026 Nerry
