use super::address::page_align_up;
use super::area::MmapInfo;
use super::{MapArea, MapPermission, MapType, MemorySet, VPNRange, VirtAddr};
use crate::config::{PAGE_SIZE, USER_MMAP_BASE, USER_MMAP_LIMIT};
use crate::fs::File;
use alloc::sync::Arc;
use core::arch::asm;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryProtectError {
    Unmapped,
    AccessDenied,
}

impl MemorySet {
    pub fn from_existed_user(user_space: &MemorySet) -> MemorySet {
        let mut memory_set = Self::new_bare();
        memory_set.brk_base = user_space.brk_base;
        memory_set.brk = user_space.brk;
        memory_set.brk_limit = user_space.brk_limit;
        memory_set.brk_mapped_end = user_space.brk_mapped_end;
        memory_set.mmap_next = user_space.mmap_next;
        memory_set.map_trampoline();
        for area in &user_space.areas {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);
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

    pub fn mprotect_area(
        &mut self,
        start: usize,
        len: usize,
        permission: MapPermission,
    ) -> Result<(), MemoryProtectError> {
        if len == 0 {
            return Ok(());
        }
        if start % PAGE_SIZE != 0 {
            return Err(MemoryProtectError::Unmapped);
        }
        let Some(end) = start.checked_add(len) else {
            return Err(MemoryProtectError::Unmapped);
        };
        let start_vpn = VirtAddr::from(start).floor();
        let end_vpn = VirtAddr::from(end).floor();
        if !self.range_is_mapped_vpn(start_vpn, end_vpn) {
            return Err(MemoryProtectError::Unmapped);
        }

        if permission.contains(MapPermission::W) && !self.can_mprotect_write(start_vpn, end_vpn) {
            return Err(MemoryProtectError::AccessDenied);
        }

        self.split_area_at(start_vpn);
        self.split_area_at(end_vpn);

        let mut touched = false;
        for area in &mut self.areas {
            let area_start = area.vpn_range.get_start();
            let area_end = area.vpn_range.get_end();
            if area_start >= start_vpn && area_end <= end_vpn {
                if !area.remap_permission(&mut self.page_table, permission) {
                    return Err(MemoryProtectError::Unmapped);
                }
                touched = true;
            }
        }
        if !touched {
            return Err(MemoryProtectError::Unmapped);
        }
        unsafe {
            asm!("sfence.vma");
        }
        Ok(())
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

    fn range_is_mapped_vpn(&self, start: super::VirtPageNum, end: super::VirtPageNum) -> bool {
        let mut cursor = start;
        while cursor < end {
            let Some(area_end) = self
                .areas
                .iter()
                .filter_map(|area| {
                    let area_start = area.vpn_range.get_start();
                    let area_end = area.vpn_range.get_end();
                    if area_start <= cursor && cursor < area_end {
                        Some(area_end)
                    } else {
                        None
                    }
                })
                .max()
            else {
                return false;
            };
            if area_end <= cursor {
                return false;
            }
            cursor = area_end.min(end);
        }
        true
    }

    fn split_area_at(&mut self, at: super::VirtPageNum) {
        let Some(idx) = self.areas.iter().position(|area| {
            let area_start = area.vpn_range.get_start();
            let area_end = area.vpn_range.get_end();
            area_start < at && at < area_end
        }) else {
            return;
        };
        if let Some(right) = self.areas[idx].split_off(at) {
            self.areas.insert(idx + 1, right);
        }
    }

    fn can_mprotect_write(&self, start: super::VirtPageNum, end: super::VirtPageNum) -> bool {
        self.areas
            .iter()
            .filter(|area| {
                let area_start = area.vpn_range.get_start();
                let area_end = area.vpn_range.get_end();
                area_start < end && area_end > start
            })
            .all(|area| {
                let Some(info) = &area.mmap_info else {
                    return true;
                };
                if !info.shared {
                    return true;
                }
                info.backing_file
                    .as_ref()
                    .is_none_or(|file| file.writable())
            })
    }
}
