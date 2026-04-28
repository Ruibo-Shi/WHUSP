use super::UPIntrFreeCell;
use crate::task::{TaskControlBlock, block_current_and_run_next, current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

pub struct SleepMutex<T> {
    data: UnsafeCell<T>,
    inner: UPIntrFreeCell<SleepMutexInner>,
}

struct SleepMutexInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

pub struct SleepMutexGuard<'a, T> {
    mutex: &'a SleepMutex<T>,
}

unsafe impl<T: Send> Send for SleepMutex<T> {}
unsafe impl<T: Send> Sync for SleepMutex<T> {}

impl<T> SleepMutex<T> {
    pub fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            inner: unsafe {
                UPIntrFreeCell::new(SleepMutexInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    pub fn lock(&self) -> SleepMutexGuard<'_, T> {
        let mut inner = self.inner.exclusive_access();
        if inner.locked {
            inner
                .wait_queue
                .push_back(current_task().expect("SleepMutex contention requires a current task"));
            drop(inner);
            block_current_and_run_next();
        } else {
            inner.locked = true;
        }
        SleepMutexGuard { mutex: self }
    }
}

impl<T> Drop for SleepMutexGuard<'_, T> {
    fn drop(&mut self) {
        let waking_task = self.mutex.inner.exclusive_session(|inner| {
            assert!(inner.locked);
            if let Some(task) = inner.wait_queue.pop_front() {
                Some(task)
            } else {
                inner.locked = false;
                None
            }
        });
        if let Some(task) = waking_task {
            wakeup_task(task);
        }
    }
}

impl<T> Deref for SleepMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for SleepMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.data.get() }
    }
}
