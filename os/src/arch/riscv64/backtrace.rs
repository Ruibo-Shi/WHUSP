use core::arch::asm;

pub fn frame_pointer() -> usize {
    let fp;
    unsafe {
        asm!("mv {}, s0", out(reg) fp);
    }
    fp
}
