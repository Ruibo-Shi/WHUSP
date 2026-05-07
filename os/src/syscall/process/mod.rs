mod clone;
mod exec;
mod id;
mod identity;
mod resource;

use crate::fs::{File, OpenFlags, open_file_in};
use crate::mm::{elf_required_interpreter_path, translated_refmut};
use crate::sbi::shutdown;
use crate::task::{
    CAP_SETPCAP, CloneArgs, CloneFlags, ProcessCpuTimesSnapshot, RLimit, RLimitResource,
    SignalFlags, SignalInfo, add_task, clone_current_thread, current_process, current_task,
    current_user_token, exit_current_and_run_next, exit_current_group_and_run_next, pid2process,
    processes_snapshot, queue_signal_to_task, suspend_current_and_run_next, wakeup_task,
};
use crate::timer::{get_time_clock_ticks, us_to_clock_ticks};
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::str;

use super::errno::{SysError, SysResult};
use super::user_ptr::{
    PATH_MAX, copy_to_user, read_user_c_string, read_user_usize, read_user_value, write_user_value,
};

pub use clone::sys_clone;
pub use exec::sys_execve;
pub use id::{
    sys_exit, sys_exit_group, sys_getpgid, sys_getpid, sys_getppid, sys_gettid, sys_kill,
    sys_sched_yield, sys_set_tid_address, sys_setpgid, sys_setsid,
};
pub use identity::{
    LinuxCapUserData, LinuxCapUserHeader, sys_capget, sys_capset, sys_getegid, sys_geteuid,
    sys_getgid, sys_getgroups, sys_getresgid, sys_getresuid, sys_getuid, sys_prctl, sys_setfsgid,
    sys_setfsuid, sys_setgid, sys_setgroups, sys_setregid, sys_setresgid, sys_setresuid,
    sys_setreuid, sys_setuid,
};
pub use resource::{
    LinuxTimeVal, LinuxTimezone, LinuxTms, LinuxUtsName, sys_getrandom, sys_getrlimit,
    sys_gettimeofday, sys_prlimit64, sys_reboot, sys_setrlimit, sys_syslog, sys_times, sys_uname,
};
