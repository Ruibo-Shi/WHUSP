#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::brk;

const PAGE_SIZE: usize = 0x1000;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let p0 = brk(0);
    let p1 = brk(p0 + PAGE_SIZE + 16);
    assert_eq!(p1, p0 + PAGE_SIZE + 16);

    unsafe {
        let ptr = p0 as *mut u8;
        for offset in 0..(PAGE_SIZE + 16) {
            ptr.add(offset).write_volatile((offset & 0xff) as u8);
        }
    }

    let p2 = brk(p0);
    assert_eq!(p2, p0);
    println!("brk_smoke passed.");
    0
}
