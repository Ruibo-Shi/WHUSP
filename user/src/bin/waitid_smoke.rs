#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{P_PID, SigInfo, WEXITED, WNOWAIT, exit, fork, waitid, waitpid};

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let pid = fork();
    if pid == 0 {
        exit(7);
    }

    let mut info = SigInfo::default();
    assert_eq!(waitid(P_PID, pid as i32, &mut info, WEXITED | WNOWAIT), 0);
    assert_eq!(info.si_signo, 17);
    assert_eq!(info.si_code, 1);
    assert_eq!(info.si_pid, pid as i32);
    assert_eq!(info.si_status, 7);

    let mut exit_code = 0;
    assert_eq!(waitpid(pid as usize, &mut exit_code), pid);
    assert_eq!(exit_code, 7);
    println!("waitid smoke passed.");
    0
}
