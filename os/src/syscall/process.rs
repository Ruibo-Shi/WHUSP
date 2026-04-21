use crate::fs::{OpenFlags, open_file_at};
use crate::mm::{translated_ref, translated_refmut, translated_str};
use crate::task::{
    CloneFlags, SignalFlags, TaskControlBlock, add_task, current_process, current_task,
    current_user_token, exit_current_and_run_next, pid2process, suspend_current_and_run_next,
};
use crate::timer::get_time_ms;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().process.upgrade().unwrap().getpid() as isize
}

pub fn sys_clone(flags: usize, stack: usize, ptid: usize, tls: usize, ctid: usize) -> isize {
    let exit_signal = (flags & 0xff) as u32;
    // Only SIGCHLD(17) or 0 supported; other termination signals not wired yet.
    if exit_signal != 0 && exit_signal != 17 {
        return -1;
    }
    let clone_flags = CloneFlags::from_bits_truncate((flags & !0xff) as u32);
    if clone_flags.contains(CloneFlags::CLONE_THREAD) {
        sys_clone_thread(clone_flags, stack, ptid, tls, ctid)
    } else {
        sys_clone_process(clone_flags, stack, ptid, tls, ctid)
    }
}

fn sys_clone_process(
    flags: CloneFlags,
    stack: usize,
    ptid: usize,
    tls: usize,
    ctid: usize,
) -> isize {
    let current_process = current_process();
    let new_process = current_process.fork();
    let new_pid = new_process.getpid();
    // Configure child's trap context (it returns immediately after scheduling).
    let new_process_inner = new_process.inner_exclusive_access();
    let task = new_process_inner.tasks[0].as_ref().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let trap_cx = task_inner.get_trap_cx();
    // Child returns 0 from clone.
    trap_cx.set_a0(0);
    if stack != 0 {
        trap_cx.set_sp(stack);
    }
    if flags.contains(CloneFlags::CLONE_SETTLS) {
        trap_cx.set_tp(tls);
    }
    if flags.contains(CloneFlags::CLONE_CHILD_CLEARTID) {
        task_inner.clear_child_tid = Some(ctid);
    }
    let child_token = new_process_inner.memory_set.token();
    drop(task_inner);
    drop(new_process_inner);
    if flags.contains(CloneFlags::CLONE_PARENT_SETTID) {
        let parent_token = current_user_token();
        *translated_refmut(parent_token, ptid as *mut i32) = new_pid as i32;
    }
    if flags.contains(CloneFlags::CLONE_CHILD_SETTID) {
        *translated_refmut(child_token, ctid as *mut i32) = new_pid as i32;
    }
    new_pid as isize
}

fn sys_clone_thread(
    flags: CloneFlags,
    stack: usize,
    ptid: usize,
    tls: usize,
    ctid: usize,
) -> isize {
    let current_task = current_task().unwrap();
    let process = current_task.process.upgrade().unwrap();
    let ustack_base = current_task
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .ustack_base;
    // Snapshot parent trap context so child resumes right after the clone syscall.
    let parent_trap_cx = *current_task.inner_exclusive_access().get_trap_cx();
    let new_task = Arc::new(TaskControlBlock::new(
        Arc::clone(&process),
        ustack_base,
        true,
    ));
    let mut new_task_inner = new_task.inner_exclusive_access();
    let new_tid = new_task_inner.res.as_ref().unwrap().tid;
    let new_ustack_top = new_task_inner.res.as_ref().unwrap().ustack_top();
    let new_trap_cx = new_task_inner.get_trap_cx();
    *new_trap_cx = parent_trap_cx;
    new_trap_cx.set_a0(0);
    new_trap_cx.set_sp(if stack != 0 { stack } else { new_ustack_top });
    if flags.contains(CloneFlags::CLONE_SETTLS) {
        new_trap_cx.set_tp(tls);
    }
    new_trap_cx.kernel_sp = new_task.kstack.get_top();
    if flags.contains(CloneFlags::CLONE_CHILD_CLEARTID) {
        new_task_inner.clear_child_tid = Some(ctid);
    }
    drop(new_task_inner);
    // Attach to parent's PCB at tasks[tid].
    let mut process_inner = process.inner_exclusive_access();
    let tasks = &mut process_inner.tasks;
    while tasks.len() < new_tid + 1 {
        tasks.push(None);
    }
    tasks[new_tid] = Some(Arc::clone(&new_task));
    let process_token = process_inner.memory_set.token();
    drop(process_inner);
    if flags.contains(CloneFlags::CLONE_PARENT_SETTID) {
        *translated_refmut(process_token, ptid as *mut i32) = new_tid as i32;
    }
    if flags.contains(CloneFlags::CLONE_CHILD_SETTID) {
        *translated_refmut(process_token, ctid as *mut i32) = new_tid as i32;
    }
    add_task(new_task);
    new_tid as isize
}

pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    let process = current_process();
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
    }
    if let Some(app_inode) = open_file_at(process.working_dir(), path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let argc = args_vec.len();
        process.exec(all_data.as_slice(), args_vec);
        // return argc because cx.x[10] will be covered with it later
        argc as isize
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let process = current_process();
    // find a child process

    let mut inner = process.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

pub fn sys_kill(pid: usize, signal: u32) -> isize {
    if let Some(process) = pid2process(pid) {
        if let Some(flag) = SignalFlags::from_bits(signal) {
            process.inner_exclusive_access().signals |= flag;
            0
        } else {
            -1
        }
    } else {
        -1
    }
}
