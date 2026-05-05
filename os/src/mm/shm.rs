use super::{FrameTracker, MapPermission, PhysPageNum, frame_alloc};
use crate::config::PAGE_SIZE;
use crate::sync::UPIntrFreeCell;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lazy_static::*;

pub(crate) const IPC_PRIVATE: isize = 0;
pub(crate) const IPC_CREAT: i32 = 0o1000;
pub(crate) const IPC_EXCL: i32 = 0o2000;
pub(crate) const IPC_RMID: i32 = 0;
pub(crate) const SHM_RDONLY: i32 = 0o10000;
pub(crate) const SHM_RND: i32 = 0o20000;
pub(crate) const SHM_EXEC: i32 = 0o100000;

const SHM_MIN: usize = 1;
const SHM_MAX: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShmError {
    NotFound,
    Exists,
    Invalid,
    NoMem,
}

#[derive(Clone, Copy)]
pub(crate) struct ShmPageMapping {
    pub(crate) page_index: usize,
    pub(crate) ppn: PhysPageNum,
}

pub(crate) struct ShmAttach {
    pub(crate) len: usize,
    pub(crate) pages: Vec<ShmPageMapping>,
}

struct ShmSegment {
    key: isize,
    size: usize,
    aligned_len: usize,
    _mode: u16,
    _creator_pid: usize,
    last_pid: usize,
    attach_count: usize,
    marked_for_delete: bool,
    pages: Vec<FrameTracker>,
}

impl ShmSegment {
    fn new(
        key: isize,
        size: usize,
        aligned_len: usize,
        mode: u16,
        creator_pid: usize,
    ) -> Option<Self> {
        let page_count = aligned_len / PAGE_SIZE;
        let mut pages = Vec::with_capacity(page_count);
        for _ in 0..page_count {
            pages.push(frame_alloc()?);
        }
        Some(Self {
            key,
            size,
            aligned_len,
            _mode: mode,
            _creator_pid: creator_pid,
            last_pid: creator_pid,
            attach_count: 0,
            marked_for_delete: false,
            pages,
        })
    }

    fn page_mappings(&self) -> Vec<ShmPageMapping> {
        self.pages
            .iter()
            .enumerate()
            .map(|(page_index, frame)| ShmPageMapping {
                page_index,
                ppn: frame.ppn,
            })
            .collect()
    }
}

struct ShmManager {
    next_id: usize,
    segments: BTreeMap<usize, ShmSegment>,
    keyed_segments: BTreeMap<isize, usize>,
}

impl ShmManager {
    fn new() -> Self {
        Self {
            next_id: 1,
            segments: BTreeMap::new(),
            keyed_segments: BTreeMap::new(),
        }
    }

    fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn create_segment(
        &mut self,
        key: isize,
        size: usize,
        mode: u16,
        creator_pid: usize,
    ) -> Result<usize, ShmError> {
        if !(SHM_MIN..=SHM_MAX).contains(&size) {
            return Err(ShmError::Invalid);
        }
        let aligned_len = align_up(size).ok_or(ShmError::Invalid)?;
        let shmid = self.alloc_id();
        let segment =
            ShmSegment::new(key, size, aligned_len, mode, creator_pid).ok_or(ShmError::NoMem)?;
        self.segments.insert(shmid, segment);
        if key != IPC_PRIVATE {
            self.keyed_segments.insert(key, shmid);
        }
        Ok(shmid)
    }

    fn get_or_create(
        &mut self,
        key: isize,
        size: usize,
        shmflg: i32,
        creator_pid: usize,
    ) -> Result<usize, ShmError> {
        let mode = (shmflg & 0o777) as u16;
        let flags = shmflg & !0o777;
        // UNFINISHED: huge-page flags, permission namespaces, and Linux's
        // full key lookup rules are not modeled; iozone uses IPC_PRIVATE.
        if key == IPC_PRIVATE {
            return self.create_segment(key, size, mode, creator_pid);
        }

        if let Some(shmid) = self.keyed_segments.get(&key).copied() {
            if flags & (IPC_CREAT | IPC_EXCL) == (IPC_CREAT | IPC_EXCL) {
                return Err(ShmError::Exists);
            }
            let segment = self.segments.get(&shmid).ok_or(ShmError::NotFound)?;
            if segment.size < size {
                return Err(ShmError::Invalid);
            }
            return Ok(shmid);
        }

        if flags & IPC_CREAT == 0 {
            return Err(ShmError::NotFound);
        }
        self.create_segment(key, size, mode, creator_pid)
    }

    fn attach(&mut self, shmid: usize, pid: usize) -> Result<ShmAttach, ShmError> {
        let segment = self.segments.get_mut(&shmid).ok_or(ShmError::Invalid)?;
        if segment.marked_for_delete {
            return Err(ShmError::Invalid);
        }
        segment.attach_count += 1;
        segment.last_pid = pid;
        Ok(ShmAttach {
            len: segment.aligned_len,
            pages: segment.page_mappings(),
        })
    }

    fn retain_attached(&mut self, shmid: usize, pid: usize) -> bool {
        let Some(segment) = self.segments.get_mut(&shmid) else {
            return false;
        };
        segment.attach_count += 1;
        segment.last_pid = pid;
        true
    }

    fn page_mappings(&self, shmid: usize) -> Option<Vec<ShmPageMapping>> {
        self.segments
            .get(&shmid)
            .map(ShmSegment::page_mappings)
    }

    fn detach(&mut self, shmid: usize, pid: usize) -> Result<(), ShmError> {
        let Some(segment) = self.segments.get_mut(&shmid) else {
            return Err(ShmError::Invalid);
        };
        segment.attach_count = segment.attach_count.saturating_sub(1);
        segment.last_pid = pid;
        if segment.attach_count == 0 && segment.marked_for_delete {
            let key = segment.key;
            self.segments.remove(&shmid);
            if key != IPC_PRIVATE {
                self.keyed_segments.remove(&key);
            }
        }
        Ok(())
    }

    fn mark_for_delete(&mut self, shmid: usize, pid: usize) -> Result<(), ShmError> {
        let Some(segment) = self.segments.get_mut(&shmid) else {
            return Err(ShmError::Invalid);
        };
        segment.marked_for_delete = true;
        segment.last_pid = pid;
        if segment.attach_count == 0 {
            let key = segment.key;
            self.segments.remove(&shmid);
            if key != IPC_PRIVATE {
                self.keyed_segments.remove(&key);
            }
        }
        Ok(())
    }
}

lazy_static! {
    static ref SHM_MANAGER: UPIntrFreeCell<ShmManager> =
        unsafe { UPIntrFreeCell::new(ShmManager::new()) };
}

pub(crate) fn shmget_segment(
    key: isize,
    size: usize,
    shmflg: i32,
    creator_pid: usize,
) -> Result<usize, ShmError> {
    SHM_MANAGER
        .exclusive_access()
        .get_or_create(key, size, shmflg, creator_pid)
}

pub(crate) fn attach_segment(shmid: usize, pid: usize) -> Result<ShmAttach, ShmError> {
    SHM_MANAGER.exclusive_access().attach(shmid, pid)
}

pub(crate) fn retain_attached_segment(shmid: usize, pid: usize) -> bool {
    SHM_MANAGER.exclusive_access().retain_attached(shmid, pid)
}

pub(crate) fn attached_segment_pages(shmid: usize) -> Option<Vec<ShmPageMapping>> {
    SHM_MANAGER.exclusive_access().page_mappings(shmid)
}

pub(crate) fn detach_segment(shmid: usize, pid: usize) -> Result<(), ShmError> {
    SHM_MANAGER.exclusive_access().detach(shmid, pid)
}

pub(crate) fn mark_segment_for_delete(shmid: usize, pid: usize) -> Result<(), ShmError> {
    SHM_MANAGER.exclusive_access().mark_for_delete(shmid, pid)
}

pub(crate) fn shm_permission_from_flags(shmflg: i32) -> Result<MapPermission, ShmError> {
    // UNFINISHED: SHM_RND address rounding, SHM_REMAP, SHM_LOCKED, and
    // permission checks are deferred; iozone attaches read/write at addr 0.
    let unsupported = shmflg & !(SHM_RDONLY | SHM_RND | SHM_EXEC);
    if unsupported != 0 {
        return Err(ShmError::Invalid);
    }
    let mut permission = MapPermission::U | MapPermission::R;
    if shmflg & SHM_RDONLY == 0 {
        permission |= MapPermission::W;
    }
    if shmflg & SHM_EXEC != 0 {
        permission |= MapPermission::X;
    }
    Ok(permission)
}

fn align_up(size: usize) -> Option<usize> {
    size.checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
}
