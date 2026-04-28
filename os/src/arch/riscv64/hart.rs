use crate::arch::interrupt;
use core::arch::asm;

pub fn enable_interrupt_and_wait() {
    interrupt::enable_supervisor_interrupt();
    wait_for_interrupt();
}

fn wait_for_interrupt() {
    unsafe {
        asm!("wfi");
    }
}

pub fn boot_stack_top() -> usize {
    let boot_stack_top;
    unsafe {
        asm!("la {},boot_stack_top", out(reg) boot_stack_top);
    }
    boot_stack_top
}
