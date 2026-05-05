use crate::fs::{OpenFlags, open_file};
use alloc::{string::String, vec, vec::Vec};

const BUSYBOX_PATH: &str = "/musl/busybox";
const BUSYBOX_APPLET: &str = "sh";
const BUSYBOX_COMMAND_FLAG: &str = "-c";
const TEST_LIBCS: &[&str] = &["/musl", "/glibc"];
const TEST: bool = false;
// CONTEXT: temporary - only netperf is enabled for default-run debugging;
// restore the broader suite list after this branch is validated.
const TEST_SCRIPTS: &[&str] = &[
    // perfect
    // "basic_testcode.sh",
    //runable
    // "busybox_testcode.sh",
    //perfect
    // "lua_testcode.sh",
    //runalbe
    // "libctest_testcode.sh",
    //runalbe
    // "iozone_testcode.sh",
    //runable
    // "unixbench_testcode.sh",
    //runalbe
    // "iperf_testcode.sh",
    // "libcbench_testcode.sh",
    // "lmbench_testcode.sh",
    //runalbe
    "netperf_testcode.sh",
    //runalbe
    // "cyclictest_testcode.sh",
    // "ltp_testcode.sh",
];

pub(super) struct KernelInitProc {
    pub(super) path: String,
    pub(super) data: Vec<u8>,
    pub(super) argv: Vec<String>,
    pub(super) envp: Vec<String>,
}

fn build_runner_command() -> String {
    let mut command = String::new();
    if TEST_SCRIPTS.is_empty() || TEST {
        command.push_str("/musl/busybox sh");
    } else {
        let mut first = true;
        for script in TEST_SCRIPTS {
            for libc_root in TEST_LIBCS {
                if !first {
                    command.push_str("; ");
                }
                command.push_str("cd ");
                command.push_str(libc_root);
                command.push_str(" && ./busybox sh ./");
                command.push_str(script);
                first = false;
            }
        }
        command.push_str("; cd /musl && ./busybox reboot -f");
    }
    command
}

pub(super) fn load() -> Option<KernelInitProc> {
    let inode = open_file(BUSYBOX_PATH, OpenFlags::RDONLY).ok()?;
    Some(KernelInitProc {
        path: BUSYBOX_PATH.into(),
        data: inode.read_all(),
        argv: vec![
            BUSYBOX_PATH.into(),
            BUSYBOX_APPLET.into(),
            BUSYBOX_COMMAND_FLAG.into(),
            build_runner_command(),
        ],
        envp: vec![
            "PATH=/:/bin:/sbin:/usr/bin:/usr/local/bin".into(),
            "LD_LIBRARY_PATH=/glibc/lib:/musl/lib:/lib".into(),
        ],
    })
}
