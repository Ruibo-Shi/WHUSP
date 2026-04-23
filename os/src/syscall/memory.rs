use crate::task::current_process;

use super::errno::SysResult;

pub fn sys_brk(addr: usize) -> SysResult {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    Ok(inner.memory_set.set_program_break(addr) as isize)
}
