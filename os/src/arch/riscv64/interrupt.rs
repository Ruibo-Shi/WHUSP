use riscv::register::sstatus;

pub fn supervisor_interrupt_enabled() -> bool {
    sstatus::read().sie()
}

pub fn enable_supervisor_interrupt() {
    unsafe {
        sstatus::set_sie();
    }
}

pub fn disable_supervisor_interrupt() {
    unsafe {
        sstatus::clear_sie();
    }
}
