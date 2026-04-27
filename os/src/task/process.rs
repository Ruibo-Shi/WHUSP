use super::id::RecycleAllocator;
use super::{FD_LIMIT, FdTableEntry, PidHandle, SignalFlags, TaskControlBlock};
use crate::fs::WorkingDir;
use crate::mm::MemorySet;
use crate::sync::{Condvar, Mutex, Semaphore, UPIntrFreeCell, UPIntrRefMut};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

pub struct ProcessControlBlock {
    // immutable
    pub pid: PidHandle,
    // mutable
    pub(super) inner: UPIntrFreeCell<ProcessControlBlockInner>,
}

pub struct ProcessControlBlockInner {
    pub is_zombie: bool,
    pub memory_set: MemorySet,
    pub cwd: WorkingDir,
    pub cwd_path: String,
    pub parent: Option<Weak<ProcessControlBlock>>,
    pub children: Vec<Arc<ProcessControlBlock>>,
    pub exit_code: i32,
    pub fd_table: Vec<Option<FdTableEntry>>,
    pub signals: SignalFlags,
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    pub task_res_allocator: RecycleAllocator,
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
}

impl ProcessControlBlockInner {
    #[allow(unused)]
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    pub fn alloc_fd(&mut self) -> usize {
        self.alloc_fd_from(0).expect("fd table exhausted")
    }

    pub fn alloc_fd_from(&mut self, lower_bound: usize) -> Option<usize> {
        if lower_bound >= FD_LIMIT {
            return None;
        }
        if let Some(fd) =
            (lower_bound..self.fd_table.len().min(FD_LIMIT)).find(|fd| self.fd_table[*fd].is_none())
        {
            Some(fd)
        } else {
            let fd = self.fd_table.len().max(lower_bound);
            if fd >= FD_LIMIT {
                return None;
            }
            while self.fd_table.len() <= fd {
                self.fd_table.push(None);
            }
            Some(fd)
        }
    }

    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }

    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }

    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }
}

impl ProcessControlBlock {
    pub fn inner_exclusive_access(&self) -> UPIntrRefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn working_dir(&self) -> WorkingDir {
        self.inner.exclusive_access().cwd
    }

    pub fn working_dir_path(&self) -> String {
        self.inner.exclusive_access().cwd_path.clone()
    }

    pub fn set_working_dir(&self, cwd: WorkingDir, cwd_path: String) {
        let mut inner = self.inner.exclusive_access();
        inner.cwd = cwd;
        inner.cwd_path = cwd_path;
    }

    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    pub fn parent_process(&self) -> Option<Arc<Self>> {
        self.inner
            .exclusive_access()
            .parent
            .as_ref()
            .and_then(Weak::upgrade)
    }

    pub fn getppid(&self) -> usize {
        self.parent_process().map_or(0, |parent| parent.getpid())
    }
}
