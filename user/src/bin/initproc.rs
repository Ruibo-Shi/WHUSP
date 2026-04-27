#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::println;
use user_lib::{exec, fork, wait, yield_};

const SHELL_PATH: &str = "/user_shell";
const SHELL_PATH_CSTR: &str = "/user_shell\0";

#[unsafe(no_mangle)]
fn main() -> i32 {
    if fork() == 0 {
        if exec(SHELL_PATH_CSTR, &[core::ptr::null::<u8>()]) != -1 {
            return 0;
        }
        println!("initproc: failed to exec {}", SHELL_PATH);
        -1
    } else {
        loop {
            let mut exit_code: i32 = 0;
            let pid = wait(&mut exit_code);
            if pid == -1 {
                yield_();
                continue;
            }
            /*
            println!(
                "[initproc] Released a zombie process, pid={}, exit_code={}",
                pid,
                exit_code,
            );
            */
        }
    }
}
