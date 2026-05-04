#![allow(dead_code)]

use super::{FrameTracker, PhysPageNum};
use crate::config::PAGE_SIZE;
use crate::fs::MountId;
use crate::sync::UPIntrFreeCell;
use alloc::collections::BTreeMap;
use lazy_static::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct PageCacheId {
    pub(crate) mount_id: MountId,
    pub(crate) ino: u32,
}

impl PageCacheId {
    pub(crate) fn new(mount_id: MountId, ino: u32) -> Self {
        Self { mount_id, ino }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct PageCacheKey {
    pub(crate) id: PageCacheId,
    pub(crate) page_index: usize,
}

impl PageCacheKey {
    pub(crate) fn from_file_offset(id: PageCacheId, file_offset: usize) -> Option<Self> {
        if file_offset % PAGE_SIZE != 0 {
            return None;
        }
        Some(Self {
            id,
            page_index: file_offset / PAGE_SIZE,
        })
    }

    pub(crate) fn file_offset(self) -> usize {
        self.page_index * PAGE_SIZE
    }
}

pub(crate) struct PageCachePage {
    pub(crate) frame: FrameTracker,
    pub(crate) key: PageCacheKey,
    pub(crate) file_size_at_load: usize,
    pub(crate) dirty: bool,
    pub(crate) ref_count: usize,
}

impl PageCachePage {
    pub(crate) fn new(frame: FrameTracker, key: PageCacheKey, file_size_at_load: usize) -> Self {
        Self {
            frame,
            key,
            file_size_at_load,
            dirty: false,
            ref_count: 0,
        }
    }

    pub(crate) fn ppn(&self) -> PhysPageNum {
        self.frame.ppn
    }
}

pub(crate) struct PageCache {
    pages: BTreeMap<PageCacheKey, PageCachePage>,
}

impl PageCache {
    pub(crate) fn new() -> Self {
        Self {
            pages: BTreeMap::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub(crate) fn contains(&self, key: PageCacheKey) -> bool {
        self.pages.contains_key(&key)
    }

    pub(crate) fn insert_loaded_page(
        &mut self,
        key: PageCacheKey,
        frame: FrameTracker,
        file_size_at_load: usize,
    ) -> PhysPageNum {
        if let Some(page) = self.pages.get(&key) {
            return page.ppn();
        }
        let page = PageCachePage::new(frame, key, file_size_at_load);
        let ppn = page.ppn();
        self.pages.insert(key, page);
        ppn
    }
}

lazy_static! {
    pub(crate) static ref PAGE_CACHE: UPIntrFreeCell<PageCache> =
        unsafe { UPIntrFreeCell::new(PageCache::new()) };
}
