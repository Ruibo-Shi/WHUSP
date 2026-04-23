use crate::mm::translated_refmut;
use crate::task::{block_current_and_run_next, current_process};
use alloc::sync::Arc;

const WNOHANG: i32 = 1;
const WUNTRACED: i32 = 2;
const WEXITED: i32 = 4;
const WCONTINUED: i32 = 8;
const WNOWAIT: i32 = 0x01000000;

const P_ALL: i32 = 0;
const P_PID: i32 = 1;

const SIGCHLD: i32 = 17;
const CLD_EXITED: i32 = 1;
const CLD_KILLED: i32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TimeVal {
    sec: isize,
    usec: isize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RUsage {
    utime: TimeVal,
    stime: TimeVal,
    maxrss: isize,
    ixrss: isize,
    idrss: isize,
    isrss: isize,
    minflt: isize,
    majflt: isize,
    nswap: isize,
    inblock: isize,
    oublock: isize,
    msgsnd: isize,
    msgrcv: isize,
    nsignals: isize,
    nvcsw: isize,
    nivcsw: isize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LinuxSigInfo {
    si_signo: i32,
    si_errno: i32,
    si_code: i32,
    si_trapno: i32,
    si_pid: i32,
    si_uid: u32,
    si_status: i32,
    si_utime: u32,
    si_stime: u32,
    si_value: u64,
    pad: [u32; 20],
    align: [u64; 0],
}

fn wait_status(exit_code: i32) -> i32 {
    if exit_code < 0 {
        exit_code
    } else {
        (exit_code & 0xff) << 8
    }
}

fn waitid_code_and_status(exit_code: i32) -> (i32, i32) {
    if exit_code < 0 {
        (CLD_KILLED, -exit_code)
    } else {
        (CLD_EXITED, exit_code & 0xff)
    }
}

fn wait4_child_matches(child_pid: usize, pid: isize) -> bool {
    pid == -1 || (pid > 0 && child_pid == pid as usize)
}

fn waitid_child_matches(child_pid: usize, idtype: i32, id: i32) -> bool {
    idtype == P_ALL || (idtype == P_PID && child_pid == id as usize)
}

fn write_rusage(token: usize, rusage: *mut RUsage) {
    if !rusage.is_null() {
        *translated_refmut(token, rusage) = RUsage::default();
    }
}

pub fn sys_wait4(pid: isize, wstatus: *mut i32, options: i32, rusage: *mut RUsage) -> isize {
    if options < 0 || options & !(WNOHANG | WUNTRACED | WCONTINUED) != 0 {
        return -1;
    }
    if pid == 0 || pid < -1 {
        return -1;
    }

    loop {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        if !inner
            .children
            .iter()
            .any(|child| wait4_child_matches(child.getpid(), pid))
        {
            return -1;
        }

        let zombie = inner.children.iter().enumerate().find(|(_, child)| {
            wait4_child_matches(child.getpid(), pid) && child.inner_exclusive_access().is_zombie
        });
        if let Some((idx, child)) = zombie {
            let found_pid = child.getpid();
            let exit_code = child.inner_exclusive_access().exit_code;
            let token = inner.memory_set.token();
            if !wstatus.is_null() {
                *translated_refmut(token, wstatus) = wait_status(exit_code);
            }
            write_rusage(token, rusage);

            let child = inner.children.remove(idx);
            assert_eq!(Arc::strong_count(&child), 1);
            return found_pid as isize;
        }

        if options & WNOHANG != 0 {
            return 0;
        }
        drop(inner);
        drop(process);
        block_current_and_run_next();
    }
}

pub fn sys_waitid(
    idtype: i32,
    id: i32,
    infop: *mut LinuxSigInfo,
    options: i32,
    rusage: *mut RUsage,
) -> isize {
    if options < 0
        || options & !(WNOHANG | WEXITED | WNOWAIT | WUNTRACED | WCONTINUED) != 0
        || options & WEXITED == 0
    {
        return -1;
    }
    if idtype != P_ALL && idtype != P_PID {
        return -1;
    }
    if idtype == P_PID && id <= 0 {
        return -1;
    }

    loop {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        if !inner
            .children
            .iter()
            .any(|child| waitid_child_matches(child.getpid(), idtype, id))
        {
            return -1;
        }

        let zombie = inner.children.iter().enumerate().find(|(_, child)| {
            waitid_child_matches(child.getpid(), idtype, id)
                && child.inner_exclusive_access().is_zombie
        });
        if let Some((idx, child)) = zombie {
            let child_pid = child.getpid();
            let exit_code = child.inner_exclusive_access().exit_code;
            let (si_code, si_status) = waitid_code_and_status(exit_code);
            let token = inner.memory_set.token();
            if !infop.is_null() {
                *translated_refmut(token, infop) = LinuxSigInfo {
                    si_signo: SIGCHLD,
                    si_code,
                    si_pid: child_pid as i32,
                    si_status,
                    ..LinuxSigInfo::default()
                };
            }
            write_rusage(token, rusage);

            if options & WNOWAIT == 0 {
                let child = inner.children.remove(idx);
                assert_eq!(Arc::strong_count(&child), 1);
            }
            return 0;
        }

        if options & WNOHANG != 0 {
            let token = inner.memory_set.token();
            if !infop.is_null() {
                *translated_refmut(token, infop) = LinuxSigInfo::default();
            }
            write_rusage(token, rusage);
            return 0;
        }
        drop(inner);
        drop(process);
        block_current_and_run_next();
    }
}
