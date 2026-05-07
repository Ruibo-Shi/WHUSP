#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxTimeSpec {
    pub(in crate::syscall) tv_sec: isize,
    pub(in crate::syscall) tv_nsec: isize,
}
