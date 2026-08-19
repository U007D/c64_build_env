/* Linker script for mos-c64-none. build.rs picks this from the target vendor;
 * see memory-mega65.x for the other machine.
 *
 * A C64 PRG loads at $0801, so fixed placements must sit above the BASIC header
 * the SDK emits there.
 */

INCLUDE link.ld

SECTIONS {
  # specify section symbols here, e.g.
  # // in your Rust code:
  # #[unsafe(no_mangle)]
  # #[unsafe(link_section = ".my_data")]
  # pub static MY_DATA: [u8; MY_DATA_SIZE] = *include_bytes!("../data/my_data.bin");

  # Uncomment below and change symbol names to match your code.
  # $2000 is the VIC-II character base: the VIC-II sees only 16K at a time and
  # reads character data at a fixed offset in that bank, so it is a hardware
  # constraint. $c000 is ordinary RAM.
  # .my_data    0x2000 : { KEEP(*(.my_data))  }
  # .my_data_2  0xc000 : { KEEP(*(.my_data_2))  }
} INSERT AFTER .data;
