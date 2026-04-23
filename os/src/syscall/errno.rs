#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(isize)]
#[allow(dead_code)]
pub enum SysError {
    EPERM = 1,
    ENOENT = 2,
    ESRCH = 3,
    EINTR = 4,
    EIO = 5,
    ENOEXEC = 8,
    EBADF = 9,
    ECHILD = 10,
    EAGAIN = 11,
    ENOMEM = 12,
    EACCES = 13,
    EFAULT = 14,
    ENOTDIR = 20,
    EINVAL = 22,
    ENOSYS = 38,
    ENOTEMPTY = 39,
    ERANGE = 34,
}

pub type SysResult<T = isize> = Result<T, SysError>;

pub fn ret(result: SysResult<isize>) -> isize {
    match result {
        Ok(value) => value,
        Err(err) => -(err as isize),
    }
}
