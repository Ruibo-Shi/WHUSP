use crate::config::PAGE_SIZE;
use crate::mm::MapPermission;
use crate::task::current_process;

use super::errno::{SysError, SysResult};

const PROT_READ: usize = 0x1;
const PROT_WRITE: usize = 0x2;
const PROT_EXEC: usize = 0x4;
const PROT_MASK: usize = PROT_READ | PROT_WRITE | PROT_EXEC;

const MAP_SHARED: usize = 0x01;
const MAP_PRIVATE: usize = 0x02;
const MAP_ANONYMOUS: usize = 0x20;
const MAP_SUPPORTED: usize = MAP_SHARED | MAP_PRIVATE | MAP_ANONYMOUS;
const MAP_TYPE_MASK: usize = 0x03;

pub fn sys_brk(addr: usize) -> SysResult {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    Ok(inner.memory_set.set_program_break(addr) as isize)
}

pub fn sys_mmap(
    _addr: usize,
    len: usize,
    prot: usize,
    flags: usize,
    fd: usize,
    offset: usize,
) -> SysResult {
    Ok(sys_mmap_impl(len, prot, flags, fd, offset)
        .map(|addr| addr as isize)
        .unwrap_or(-1))
}

// TODO: prot ... i don't think this is a good name
fn sys_mmap_impl(
    len: usize,
    prot: usize,
    flags: usize,
    fd: usize,
    offset: usize,
) -> Result<usize, SysError> {
    if len == 0 || offset % PAGE_SIZE != 0 {
        return Err(SysError::EINVAL);
    }
    if prot & !PROT_MASK != 0 {
        return Err(SysError::EINVAL);
    }
    if flags & !MAP_SUPPORTED != 0 {
        return Err(SysError::EINVAL);
    }
    let map_type = flags & MAP_TYPE_MASK;
    if map_type != MAP_SHARED && map_type != MAP_PRIVATE {
        return Err(SysError::EINVAL);
    }

    let shared = map_type == MAP_SHARED;
    let anonymous = flags & MAP_ANONYMOUS != 0;
    let writable = prot & PROT_WRITE != 0;
    let mut permission = MapPermission::U;
    if prot & PROT_READ != 0 || writable {
        permission |= MapPermission::R;
    }
    if writable {
        permission |= MapPermission::W;
    }
    if prot & PROT_EXEC != 0 {
        permission |= MapPermission::X;
    }

    let process = current_process();
    let backing_file = if anonymous {
        None
    } else {
        let fd = fd as isize;
        if fd < 0 {
            return Err(SysError::EBADF);
        }
        let inner = process.inner_exclusive_access();
        let file = inner
            .fd_table
            .get(fd as usize)
            .and_then(|entry| entry.as_ref())
            .map(|entry| entry.file())
            .ok_or(SysError::EBADF)?;
        if !file.readable() {
            return Err(SysError::EACCES);
        }
        if shared && writable && !file.writable() {
            return Err(SysError::EACCES);
        }
        Some(file)
    };

    let mut inner = process.inner_exclusive_access();
    // TODO: why dose map permission do not contain shared and writable
    inner
        .memory_set
        .mmap_area(len, permission, backing_file, offset, shared, writable)
        .ok_or(SysError::ENOMEM)
}

pub fn sys_munmap(addr: usize, len: usize) -> SysResult {
    if len == 0 || addr % PAGE_SIZE != 0 {
        return Err(SysError::EINVAL);
    }
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if inner.memory_set.munmap_area(addr, len) {
        Ok(0)
    } else {
        Err(SysError::EINVAL)
    }
}
