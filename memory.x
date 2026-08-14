INCLUDE link.ld

SECTIONS {
  # specify section symbols here, e.g.
  # // in your Rust code:
  # #[unsafe(no_mangle)]
  # #[unsafe(link_section = ".my_data")]
  # pub static MY_DATA: [u8; MY_DATA_SIZE] = *include_bytes!("../data/my_data.bin");

  # Uncomment below and change symbol names to match your code
  # .my_data    0x2000 : { KEEP(*(.my_data))  }
  # .my_data_2  0xc000 : { KEEP(*(.my_data_2))  }
} INSERT AFTER .data;
