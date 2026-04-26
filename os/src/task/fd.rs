use crate::fs::{File, OpenFlags};
use crate::sync::UPIntrFreeCell;
use alloc::sync::Arc;
use bitflags::bitflags;

pub const FD_LIMIT: usize = 1024;

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FdFlags: u32 {
        const CLOEXEC = 1;
    }
}

pub struct FileDescription {
    file: Arc<dyn File + Send + Sync>,
    status_flags: UPIntrFreeCell<OpenFlags>,
}

#[derive(Clone)]
pub struct FdTableEntry {
    description: Arc<FileDescription>,
    fd_flags: FdFlags,
}

impl FdTableEntry {
    pub fn from_file(file: Arc<dyn File + Send + Sync>, open_flags: OpenFlags) -> Self {
        let fd_flags = if open_flags.contains(OpenFlags::CLOEXEC) {
            FdFlags::CLOEXEC
        } else {
            FdFlags::empty()
        };
        Self::new(file, OpenFlags::file_status_flags(open_flags), fd_flags)
    }

    pub fn new(
        file: Arc<dyn File + Send + Sync>,
        status_flags: OpenFlags,
        fd_flags: FdFlags,
    ) -> Self {
        Self {
            description: Arc::new(FileDescription {
                file,
                status_flags: unsafe { UPIntrFreeCell::new(status_flags) },
            }),
            fd_flags,
        }
    }

    pub fn duplicate(&self, fd_flags: FdFlags) -> Self {
        Self {
            description: Arc::clone(&self.description),
            fd_flags,
        }
    }

    pub fn file(&self) -> Arc<dyn File + Send + Sync> {
        Arc::clone(&self.description.file)
    }

    pub fn fd_flags(&self) -> FdFlags {
        self.fd_flags
    }

    pub fn set_fd_flags(&mut self, flags: FdFlags) {
        self.fd_flags = flags;
    }

    pub fn status_flags(&self) -> OpenFlags {
        *self.description.status_flags.exclusive_access()
    }

    pub fn set_status_flags(&self, flags: OpenFlags) {
        *self.description.status_flags.exclusive_access() = flags;
    }

    pub fn close_on_exec(&self) -> bool {
        self.fd_flags.contains(FdFlags::CLOEXEC)
    }
}
