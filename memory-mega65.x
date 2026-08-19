/* Linker script for mos-mega65-none. build.rs picks this from the target vendor;
 * see memory-c64.x for the other machine.
 *
 * MEGA65 memory layout, bank 0 (from the SDK's mega65/lib/link.ld). The linker
 * region is $2001-$cfff; the soft stack starts at $d000 and grows down:
 *
 *   $2001-$9fff  free RAM — the program loads at $2001
 *   $a000-$bfff  BASIC ROM, switched to RAM by the SDK's unmap-basic
 *   $c000-$cfff  free RAM
 *   $d000-$dfff  I/O
 *   $e000-$ffff  KERNAL
 */

INCLUDE link.ld

SECTIONS {
  # Mirror the sections in memory-c64.x. Addresses need not match — pick from the
  # map above, or omit the address entirely and let the linker place the section.
  # .my_data    : { KEEP(*(.my_data))  }
  # .my_data_2  0xc000 : { KEEP(*(.my_data_2))  }
} INSERT AFTER .data;
