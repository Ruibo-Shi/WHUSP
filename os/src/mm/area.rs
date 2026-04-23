use super::frame_alloc;
use super::page_table::PTEFlags;
use super::{FrameTracker, PageTable, PhysPageNum, StepByOne, VPNRange, VirtAddr, VirtPageNum};
use crate::config::PAGE_SIZE;
use crate::fs::File;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;

pub struct MapArea {
    pub(super) vpn_range: VPNRange,
    pub(super) data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    pub(super) map_type: MapType,
    pub(super) map_perm: MapPermission,
    pub(super) mmap_info: Option<MmapInfo>,
}

#[derive(Clone)]
pub(super) struct MmapInfo {
    pub(super) shared: bool,
    pub(super) writable: bool,
    pub(super) len: usize,
    pub(super) file_offset: usize,
    pub(super) backing_file: Option<Arc<dyn File + Send + Sync>>,
}

impl MapArea {
    pub(super) fn new(
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

    pub(super) fn from_another(another: &MapArea) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
            mmap_info: another.mmap_info.clone(),
        }
    }

    pub(super) fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum = match self.map_type {
            MapType::Identical => PhysPageNum(vpn.0),
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                let ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
                ppn
            }
            MapType::Linear(pn_offset) => {
                assert!(vpn.0 < (1usize << 27));
                PhysPageNum((vpn.0 as isize + pn_offset) as usize)
            }
        };
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits()).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }

    pub(super) fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }

    pub(super) fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }

    pub(super) fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }

    pub(super) fn copy_data(&mut self, page_table: &PageTable, data: &[u8], data_offset: usize) {
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

    pub(super) fn is_mmap(&self) -> bool {
        self.mmap_info.is_some()
    }

    pub(super) fn load_mmap_data(&self, page_table: &PageTable) {
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

    pub(super) fn flush_mmap_data(&self, page_table: &PageTable) {
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
