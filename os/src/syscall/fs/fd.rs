use crate::fs::{File, OpenFlags, make_pipe};
use crate::mm::translated_refmut;
use crate::task::{FD_LIMIT, FdFlags, FdTableEntry, current_process, current_user_token};
use alloc::sync::Arc;

use super::super::errno::{SysError, SysResult};
use super::user_ptr::{read_user_value, write_user_value};

const F_DUPFD: usize = 0;
const F_GETFD: usize = 1;
const F_SETFD: usize = 2;
const F_GETFL: usize = 3;
const F_SETFL: usize = 4;
const F_GETLK: usize = 5;
const F_SETLK: usize = 6;
const F_SETLKW: usize = 7;
const F_DUPFD_CLOEXEC: usize = 1030;

const F_RDLCK: i16 = 0;
const F_WRLCK: i16 = 1;
const F_UNLCK: i16 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxFlock {
    l_type: i16,
    l_whence: i16,
    l_start: i64,
    l_len: i64,
    l_pid: i32,
}

pub(super) fn get_fd_entry_by_fd(fd: usize) -> SysResult<FdTableEntry> {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    inner
        .fd_table
        .get(fd)
        .and_then(|entry| entry.as_ref())
        .cloned()
        .ok_or(SysError::EBADF)
}

pub(super) fn get_file_by_fd(fd: usize) -> SysResult<Arc<dyn File + Send + Sync>> {
    Ok(get_fd_entry_by_fd(fd)?.file())
}

pub fn sys_close(fd: usize) -> SysResult {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return Err(SysError::EBADF);
    }
    if inner.fd_table[fd].is_none() {
        return Err(SysError::EBADF);
    }
    inner.fd_table[fd].take();
    Ok(0)
}

pub fn sys_pipe(pipe: *mut usize) -> SysResult {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(FdTableEntry::from_file(pipe_read, OpenFlags::RDONLY));
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(FdTableEntry::from_file(pipe_write, OpenFlags::WRONLY));
    *translated_refmut(token, pipe) = read_fd;
    *translated_refmut(token, unsafe { pipe.add(1) }) = write_fd;
    Ok(0)
}

pub fn sys_dup(fd: usize) -> SysResult {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let entry = inner
        .fd_table
        .get(fd)
        .and_then(|entry| entry.as_ref())
        .cloned()
        .ok_or(SysError::EBADF)?;
    let new_fd = inner.alloc_fd();
    inner.fd_table[new_fd] = Some(entry.duplicate(FdFlags::empty()));
    Ok(new_fd as isize)
}

fn fcntl_dup(fd: usize, lower_bound: usize, fd_flags: FdFlags) -> SysResult {
    if lower_bound >= FD_LIMIT {
        return Err(SysError::EINVAL);
    }
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let entry = inner
        .fd_table
        .get(fd)
        .and_then(|entry| entry.as_ref())
        .cloned()
        .ok_or(SysError::EBADF)?;
    let new_fd = inner.alloc_fd_from(lower_bound).ok_or(SysError::EMFILE)?;
    inner.fd_table[new_fd] = Some(entry.duplicate(fd_flags));
    Ok(new_fd as isize)
}

fn valid_flock_type(l_type: i16) -> bool {
    matches!(l_type, F_RDLCK | F_WRLCK | F_UNLCK)
}

fn fcntl_getlk(fd: usize, lock: *mut LinuxFlock) -> SysResult {
    let _ = get_fd_entry_by_fd(fd)?;
    let token = current_user_token();
    let mut flock = read_user_value(token, lock.cast_const())?;
    if !valid_flock_type(flock.l_type) {
        return Err(SysError::EINVAL);
    }
    // UNFINISHED: byte-range lock conflict tracking is not implemented; report no conflict.
    flock.l_type = F_UNLCK;
    write_user_value(token, lock, &flock)?;
    Ok(0)
}

fn fcntl_setlk(fd: usize, lock: *const LinuxFlock) -> SysResult {
    let _ = get_fd_entry_by_fd(fd)?;
    let token = current_user_token();
    let flock = read_user_value(token, lock)?;
    if !valid_flock_type(flock.l_type) {
        return Err(SysError::EINVAL);
    }
    // UNFINISHED: advisory byte-range lock ownership, conflicts, and F_SETLKW waits are ignored.
    Ok(0)
}

pub fn sys_fcntl(fd: usize, op: usize, arg: usize) -> SysResult {
    match op {
        F_DUPFD => fcntl_dup(fd, arg, FdFlags::empty()),
        F_DUPFD_CLOEXEC => fcntl_dup(fd, arg, FdFlags::CLOEXEC),
        F_GETFD => Ok(get_fd_entry_by_fd(fd)?.fd_flags().bits() as isize),
        F_SETFD => {
            let process = current_process();
            let mut inner = process.inner_exclusive_access();
            let entry = inner
                .fd_table
                .get_mut(fd)
                .and_then(|entry| entry.as_mut())
                .ok_or(SysError::EBADF)?;
            entry.set_fd_flags(FdFlags::from_bits_truncate(
                (arg as u32) & FdFlags::CLOEXEC.bits(),
            ));
            Ok(0)
        }
        F_GETFL => Ok(get_fd_entry_by_fd(fd)?.status_flags().bits() as isize),
        F_SETFL => {
            let entry = get_fd_entry_by_fd(fd)?;
            let status = entry.status_flags();
            // UNFINISHED: O_DIRECT is recorded for fcntl compatibility, but direct-I/O
            // alignment and cache-bypass semantics are not enforced by the filesystem layer.
            entry.set_status_flags(status.with_fcntl_status_flags(arg as u32));
            Ok(0)
        }
        F_GETLK => fcntl_getlk(fd, arg as *mut LinuxFlock),
        F_SETLK | F_SETLKW => fcntl_setlk(fd, arg as *const LinuxFlock),
        _ => Err(SysError::EINVAL),
    }
}
