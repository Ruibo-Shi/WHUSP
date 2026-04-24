use super::*;

bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREATE = 0o100;
        const TRUNC = 0o1000;
        const DIRECTORY = 0o200000;
    }
}

const AT_FDCWD: isize = -100;

fn compat_ret(ret: isize) -> isize {
    if ret < 0 { -1 } else { ret }
}

pub fn dup(fd: usize) -> isize {
    compat_ret(sys_dup(fd))
}
pub fn ioctl(fd: usize, request: usize, argp: usize) -> isize {
    compat_ret(sys_ioctl(fd, request, argp))
}
pub fn open(path: &str, flags: OpenFlags) -> isize {
    compat_ret(sys_open(path, flags.bits))
}
pub fn openat(dirfd: isize, path: &str, flags: OpenFlags, mode: u32) -> isize {
    compat_ret(sys_openat(dirfd, path, flags.bits, mode))
}
pub fn getcwd(buf: &mut [u8]) -> isize {
    compat_ret(sys_getcwd(buf.as_mut_ptr(), buf.len()))
}
pub fn chdir(path: &str) -> isize {
    compat_ret(sys_chdir(path))
}
pub fn mkdir(path: &str, mode: u32) -> isize {
    compat_ret(sys_mkdirat(AT_FDCWD, path, mode))
}
pub fn unlink(path: &str) -> isize {
    compat_ret(sys_unlinkat(AT_FDCWD, path, 0))
}
pub fn getdents64(fd: usize, buf: &mut [u8]) -> isize {
    compat_ret(sys_getdents64(fd, buf.as_mut_ptr(), buf.len()))
}
pub fn close(fd: usize) -> isize {
    compat_ret(sys_close(fd))
}
pub fn pipe(pipe_fd: &mut [usize]) -> isize {
    compat_ret(sys_pipe(pipe_fd))
}
pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    compat_ret(sys_read(fd, buf))
}
pub fn write(fd: usize, buf: &[u8]) -> isize {
    compat_ret(sys_write(fd, buf))
}
