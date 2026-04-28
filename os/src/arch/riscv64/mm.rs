use core::arch::asm;
use riscv::register::satp;

const SV39_MODE: usize = 8;
const SATP_PPN_MASK: usize = (1usize << 44) - 1;

pub fn page_table_token(root_ppn: usize) -> usize {
    SV39_MODE << 60 | root_ppn
}

pub fn page_table_root_ppn(token: usize) -> usize {
    token & SATP_PPN_MASK
}

pub fn activate_page_table(token: usize) {
    satp::write(token);
    flush_tlb_all();
}

pub fn flush_tlb_all() {
    unsafe {
        asm!("sfence.vma");
    }
}
