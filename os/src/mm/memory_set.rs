use super::{FrameTracker, frame_alloc};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{
    PAGE_SIZE, TRAMPOLINE, USER_HEAP_SIZE, USER_MMAP_BASE, USER_MMAP_LIMIT, memory_end,
    mmio_regions,
};
use crate::fs::File;
use crate::sync::UPIntrFreeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use lazy_static::*;
use riscv::register::satp;

unsafe extern "C" {
    safe fn stext();
    safe fn etext();
    safe fn srodata();
    safe fn erodata();
    safe fn sdata();
    safe fn edata();
    safe fn sbss_with_stack();
    safe fn ebss();
    safe fn ekernel();
    safe fn strampoline();
}

lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UPIntrFreeCell<MemorySet>> =
        Arc::new(unsafe { UPIntrFreeCell::new(MemorySet::new_kernel()) });
}

pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

// TODO: replace vec to a high perfermonce data structure
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
    brk_base: usize,
    brk: usize,
    brk_limit: usize,
    brk_mapped_end: usize,
    mmap_next: usize,
}

pub struct ElfLoadInfo {
    pub memory_set: MemorySet,
    pub ustack_base: usize,
    pub entry_point: usize,
    pub phdr: usize,
    pub phent: usize,
    pub phnum: usize,
}

impl MemorySet {
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
            brk_base: 0,
            brk: 0,
            brk_limit: 0,
            brk_mapped_end: 0,
            mmap_next: USER_MMAP_BASE,
        }
    }
    pub fn token(&self) -> usize {
        self.page_table.token()
    }
    /// Assume that no conflicts.
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }
    pub fn remove_area_with_start_vpn(&mut self, start_vpn: VirtPageNum) {
        if let Some((idx, area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_, area)| area.vpn_range.get_start() == start_vpn)
        {
            area.unmap(&mut self.page_table);
            self.areas.remove(idx);
        }
    }
    /// Add a new MapArea into this MemorySet.
    /// Assuming that there are no conflicts in the virtual address
    /// space.
    pub fn push(&mut self, map_area: MapArea, data: Option<&[u8]>) {
        self.push_with_offset(map_area, data, 0);
    }

    // TODO: seem to be a extra layer of abstration
    fn push_with_offset(&mut self, mut map_area: MapArea, data: Option<&[u8]>, data_offset: usize) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&self.page_table, data, data_offset);
        }
        self.areas.push(map_area);
    }
    /// Mention that trampoline is not collected by areas.
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as usize).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }
    /// Without kernel stacks.
    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // map kernel sections
        // println!(".text [{:#x}, {:#x})", stext as usize, etext as usize);
        // println!(".rodata [{:#x}, {:#x})", srodata as usize, erodata as usize);
        // println!(".data [{:#x}, {:#x})", sdata as usize, edata as usize);
        // println!(
        //     ".bss [{:#x}, {:#x})",
        //     sbss_with_stack as usize, ebss as usize
        // );
        // println!("mapping .text section");
        memory_set.push(
            MapArea::new(
                (stext as usize).into(),
                (etext as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );
        // println!("mapping .rodata section");
        memory_set.push(
            MapArea::new(
                (srodata as usize).into(),
                (erodata as usize).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );
        // println!("mapping .data section");
        memory_set.push(
            MapArea::new(
                (sdata as usize).into(),
                (edata as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        // println!("mapping .bss section");
        memory_set.push(
            MapArea::new(
                (sbss_with_stack as usize).into(),
                (ebss as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        // println!("mapping physical memory");
        memory_set.push(
            MapArea::new(
                (ekernel as usize).into(),
                memory_end().into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        //println!("mapping memory-mapped registers");
        for pair in mmio_regions() {
            memory_set.push(
                MapArea::new(
                    pair.base.into(),
                    (pair.base + pair.size).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            );
        }
        memory_set
    }
    /// Include sections in elf and trampoline, returning metadata needed to
    /// build a Linux-style initial user stack.
    pub fn from_elf(elf_data: &[u8]) -> ElfLoadInfo {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let ph_entry_size = elf_header.pt2.ph_entry_size();
        let ph_offset = elf_header.pt2.ph_offset() as usize;
        let ph_size = ph_entry_size as usize * ph_count as usize;
        let mut phdr = 0;
        let mut max_end_va = 0usize;
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            let ph_type = ph.get_type().unwrap();
            if ph_type == xmas_elf::program::Type::Phdr {
                phdr = ph.virtual_addr() as usize;
            }
            if ph_type == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let segment_end = ph.virtual_addr() as usize + ph.mem_size() as usize;
                max_end_va = max_end_va.max(segment_end);
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                memory_set.push_with_offset(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                    start_va.page_offset(),
                );
                if phdr == 0 {
                    let load_offset = ph.offset() as usize;
                    let load_file_end = load_offset + ph.file_size() as usize;
                    if ph_offset >= load_offset && ph_offset + ph_size <= load_file_end {
                        phdr = ph.virtual_addr() as usize + (ph_offset - load_offset);
                    }
                }
            }
        }
        let heap_base = page_align_up(max_end_va + PAGE_SIZE);
        let brk_limit = heap_base + USER_HEAP_SIZE;
        memory_set.brk_base = heap_base;
        memory_set.brk = heap_base;
        memory_set.brk_limit = brk_limit;
        memory_set.brk_mapped_end = heap_base;
        memory_set.mmap_next = USER_MMAP_BASE;
        memory_set.push(
            MapArea::new(
                heap_base.into(),
                heap_base.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        let user_stack_base = brk_limit + PAGE_SIZE;
        ElfLoadInfo {
            memory_set,
            ustack_base: user_stack_base,
            entry_point: elf.header.pt2.entry_point() as usize,
            phdr,
            phent: ph_entry_size as usize,
            phnum: ph_count as usize,
        }
    }
    pub fn from_existed_user(user_space: &MemorySet) -> MemorySet {
        let mut memory_set = Self::new_bare();
        memory_set.brk_base = user_space.brk_base;
        memory_set.brk = user_space.brk;
        memory_set.brk_limit = user_space.brk_limit;
        memory_set.brk_mapped_end = user_space.brk_mapped_end;
        memory_set.mmap_next = user_space.mmap_next;
        // map trampoline
        memory_set.map_trampoline();
        // copy data sections/trap_context/user_stack
        for area in user_space.areas.iter() {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);
            // copy data from another space
            for vpn in area.vpn_range {
                let src_ppn = user_space.translate(vpn).unwrap().ppn();
                let dst_ppn = memory_set.translate(vpn).unwrap().ppn();
                dst_ppn
                    .get_bytes_array()
                    .copy_from_slice(src_ppn.get_bytes_array());
            }
        }
        memory_set
    }
    pub fn set_program_break(&mut self, addr: usize) -> usize {
        if addr == 0 {
            return self.brk;
        }
        if addr < self.brk_base || addr > self.brk_limit {
            return self.brk;
        }

        let old_mapped_end = self.brk_mapped_end;
        let new_mapped_end = page_align_up(addr);
        let heap_start_vpn = VirtAddr::from(self.brk_base).floor();
        let area_idx = self
            .areas
            .iter()
            .position(|area| area.vpn_range.get_start() == heap_start_vpn)
            .expect("heap area missing from user memory set");
        let heap_area = &mut self.areas[area_idx];

        if new_mapped_end > old_mapped_end {
            let start_vpn = VirtAddr::from(old_mapped_end).floor();
            let end_vpn = VirtAddr::from(new_mapped_end).floor();
            for vpn in VPNRange::new(start_vpn, end_vpn) {
                heap_area.map_one(&mut self.page_table, vpn);
            }
        } else if new_mapped_end < old_mapped_end {
            let start_vpn = VirtAddr::from(new_mapped_end).floor();
            let end_vpn = VirtAddr::from(old_mapped_end).floor();
            for vpn in VPNRange::new(start_vpn, end_vpn) {
                heap_area.unmap_one(&mut self.page_table, vpn);
            }
        }

        heap_area.vpn_range = VPNRange::new(heap_start_vpn, VirtAddr::from(new_mapped_end).floor());
        self.brk = addr;
        self.brk_mapped_end = new_mapped_end;
        self.brk
    }
    pub fn mmap_area(
        &mut self,
        len: usize,
        permission: MapPermission,
        backing_file: Option<Arc<dyn File + Send + Sync>>,
        file_offset: usize,
        shared: bool,
        writable: bool,
    ) -> Option<usize> {
        let len = page_align_up(len);
        let start = self.alloc_mmap_range(len)?;
        let end = start + len;
        let mut area = MapArea::new(start.into(), end.into(), MapType::Framed, permission);
        area.mmap_info = Some(MmapInfo {
            shared,
            writable,
            len,
            file_offset,
            backing_file,
        });
        area.map(&mut self.page_table);
        area.load_mmap_data(&self.page_table);
        self.areas.push(area);
        self.mmap_next = end;
        Some(start)
    }
    pub fn munmap_area(&mut self, start: usize, len: usize) -> bool {
        if len == 0 || start % PAGE_SIZE != 0 {
            return false;
        }
        let Some(end) = start.checked_add(page_align_up(len)) else {
            return false;
        };
        let start_vpn = VirtAddr::from(start).floor();
        let end_vpn = VirtAddr::from(end).floor();
        let Some(idx) = self.areas.iter().position(|area| {
            area.is_mmap()
                && area.vpn_range.get_start() == start_vpn
                && area.vpn_range.get_end() == end_vpn
        }) else {
            return false;
        };
        let mut area = self.areas.remove(idx);
        area.flush_mmap_data(&self.page_table);
        area.unmap(&mut self.page_table);
        true
    }
    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            satp::write(satp);
            asm!("sfence.vma");
        }
    }
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }
    pub fn recycle_data_pages(&mut self) {
        //*self = Self::new_bare();
        self.areas.clear();
    }

    fn alloc_mmap_range(&self, len: usize) -> Option<usize> {
        if len == 0 || len > USER_MMAP_LIMIT - USER_MMAP_BASE {
            return None;
        }
        let mut start = page_align_up(self.mmap_next.max(USER_MMAP_BASE));
        while start
            .checked_add(len)
            .is_some_and(|end| end <= USER_MMAP_LIMIT)
        {
            let end = start + len;
            if !self.range_overlaps(start, end) {
                return Some(start);
            }
            start += PAGE_SIZE;
        }
        None
    }

    fn range_overlaps(&self, start: usize, end: usize) -> bool {
        let start_vpn = VirtAddr::from(start).floor();
        let end_vpn = VirtAddr::from(end).floor();
        self.areas.iter().any(|area| {
            let area_start = area.vpn_range.get_start();
            let area_end = area.vpn_range.get_end();
            start_vpn < area_end && end_vpn > area_start
        })
    }
}

fn page_align_up(addr: usize) -> usize {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
    mmap_info: Option<MmapInfo>,
}

#[derive(Clone)]
struct MmapInfo {
    shared: bool,
    writable: bool,
    len: usize,
    file_offset: usize,
    backing_file: Option<Arc<dyn File + Send + Sync>>,
}

impl MapArea {
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
            mmap_info: None,
        }
    }
    pub fn from_another(another: &MapArea) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
            mmap_info: another.mmap_info.clone(),
        }
    }

    // TODO: maybe we need to choose only one way to do it?
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
            }
            MapType::Linear(pn_offset) => {
                // check for sv39
                assert!(vpn.0 < (1usize << 27));
                ppn = PhysPageNum((vpn.0 as isize + pn_offset) as usize);
            }
        }
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits()).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }
    /// Copy file-backed bytes into a framed area at the given page offset.
    /// Unwritten bytes stay zero-filled from frame allocation.
    pub fn copy_data(&mut self, page_table: &PageTable, data: &[u8], data_offset: usize) {
        assert_eq!(self.map_type, MapType::Framed);
        assert!(data_offset < PAGE_SIZE);
        let mut copied = 0usize;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        let mut page_offset = data_offset;
        while copied < len {
            let copy_len = (PAGE_SIZE - page_offset).min(len - copied);
            let src = &data[copied..copied + copy_len];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[page_offset..page_offset + copy_len];
            dst.copy_from_slice(src);
            copied += copy_len;
            page_offset = 0;
            current_vpn.step();
        }
    }

    fn is_mmap(&self) -> bool {
        self.mmap_info.is_some()
    }

    fn load_mmap_data(&self, page_table: &PageTable) {
        let Some(info) = &self.mmap_info else {
            return;
        };
        let Some(file) = &info.backing_file else {
            return;
        };
        let mut remaining = info.len;
        let mut file_offset = info.file_offset;
        for vpn in self.vpn_range {
            if remaining == 0 {
                break;
            }
            let copy_len = remaining.min(PAGE_SIZE);
            let dst = &mut page_table.translate(vpn).unwrap().ppn().get_bytes_array()[..copy_len];
            let read_size = file.read_at(file_offset, dst);
            if read_size < copy_len {
                break;
            }
            remaining -= copy_len;
            file_offset += copy_len;
        }
    }

    fn flush_mmap_data(&self, page_table: &PageTable) {
        let Some(info) = &self.mmap_info else {
            return;
        };
        if !info.shared || !info.writable {
            return;
        }
        let Some(file) = &info.backing_file else {
            return;
        };
        let mut remaining = info.len;
        let mut file_offset = info.file_offset;
        for vpn in self.vpn_range {
            if remaining == 0 {
                break;
            }
            let copy_len = remaining.min(PAGE_SIZE);
            let src = &page_table.translate(vpn).unwrap().ppn().get_bytes_array()[..copy_len];
            file.write_at(file_offset, src);
            remaining -= copy_len;
            file_offset += copy_len;
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType {
    Identical,
    Framed,
    /// offset of page num
    Linear(isize),
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}

#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();
    assert!(
        !kernel_space
            .page_table
            .translate(mid_text.floor())
            .unwrap()
            .writable(),
    );
    assert!(
        !kernel_space
            .page_table
            .translate(mid_rodata.floor())
            .unwrap()
            .writable(),
    );
    assert!(
        !kernel_space
            .page_table
            .translate(mid_data.floor())
            .unwrap()
            .executable(),
    );
    println!("remap_test passed!");
}
