#![no_std]
#![no_main]

mod panic;

use core::mem::size_of;

use ufmt_stdio::{println, ufmt};

const BITS_PER_BYTE: usize = 8;

#[unsafe(no_mangle)]
extern "C" fn main() {
    println!("Hello, {}-bit world, from Rust!", size_of::<usize>() * BITS_PER_BYTE);
}
