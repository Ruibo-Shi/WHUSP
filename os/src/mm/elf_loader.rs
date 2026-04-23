use super::address::page_align_up;
use super::{MapArea, MapPermission, MapType, MemorySet, VirtAddr};
use crate::config::{PAGE_SIZE, USER_HEAP_SIZE, USER_MMAP_BASE};

pub struct ElfLoadInfo {
    pub memory_set: MemorySet,
    pub ustack_base: usize,
    pub entry_point: usize,
    pub phdr: usize,
    pub phent: usize,
    pub phnum: usize,
}

impl MemorySet {
    pub fn from_elf(elf_data: &[u8]) -> ElfLoadInfo {
        let mut memory_set = Self::new_bare();
        memory_set.map_trampoline();
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
}
