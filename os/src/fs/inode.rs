use super::File;
use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;
use crate::sync::UPIntrFreeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use core::str;
use core::time::Duration;
use lazy_static::*;
use lwext4_rust::ffi::{EIO, EXT4_ROOT_INO};
use lwext4_rust::{
    BlockDevice as Ext4BlockDevice, EXT4_DEV_BSIZE, Ext4Error, Ext4Filesystem, Ext4Result,
    FsConfig, InodeType, SystemHal,
};

struct KernelHal;

impl SystemHal for KernelHal {
    fn now() -> Option<Duration> {
        None
    }
}

#[derive(Clone)]
struct KernelDisk {
    dev: Arc<crate::drivers::block::VirtIOBlock>,
}

impl Ext4BlockDevice for KernelDisk {
    fn write_blocks(&mut self, block_id: u64, buf: &[u8]) -> Ext4Result<usize> {
        let mut block_buf = [0u8; EXT4_DEV_BSIZE];
        for (index, block) in buf.chunks(EXT4_DEV_BSIZE).enumerate() {
            if block.len() != EXT4_DEV_BSIZE {
                return Err(Ext4Error::new(EIO as _, "unaligned block write"));
            }
            block_buf.copy_from_slice(block);
            self.dev.write_block(block_id as usize + index, &block_buf);
        }
        Ok(buf.len())
    }

    fn read_blocks(&mut self, block_id: u64, buf: &mut [u8]) -> Ext4Result<usize> {
        let mut block_buf = [0u8; EXT4_DEV_BSIZE];
        for (index, block) in buf.chunks_mut(EXT4_DEV_BSIZE).enumerate() {
            if block.len() != EXT4_DEV_BSIZE {
                return Err(Ext4Error::new(EIO as _, "unaligned block read"));
            }
            self.dev.read_block(block_id as usize + index, &mut block_buf);
            block.copy_from_slice(&block_buf);
        }
        Ok(buf.len())
    }

    fn num_blocks(&self) -> Ext4Result<u64> {
        Ok(self.dev.num_blocks())
    }
}

type KernelExt4Fs = Ext4Filesystem<KernelHal, KernelDisk>;

const EXT4_CONFIG: FsConfig = FsConfig { bcache_size: 256 };

struct RootFs(KernelExt4Fs);

unsafe impl Send for RootFs {}
unsafe impl Sync for RootFs {}

pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPIntrFreeCell<OSInodeInner>,
}

pub struct OSInodeInner {
    offset: usize,
    ino: u32,
}

impl OSInode {
    pub fn new(readable: bool, writable: bool, ino: u32) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPIntrFreeCell::new(OSInodeInner { offset: 0, ino }) },
        }
    }

    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer = [0u8; 512];
        let mut data = Vec::new();
        loop {
            let len = ROOT_FS.exclusive_session(|fs| {
                fs.0.read_at(inner.ino, &mut buffer, inner.offset as u64)
                    .expect("ext4 read_all failed")
            });
            if len == 0 {
                break;
            }
            inner.offset += len;
            data.extend_from_slice(&buffer[..len]);
        }
        data
    }
}

lazy_static! {
    static ref ROOT_FS: UPIntrFreeCell<RootFs> = unsafe {
        UPIntrFreeCell::new(
            RootFs(
                KernelExt4Fs::new(
                    KernelDisk {
                        dev: BLOCK_DEVICE.clone(),
                    },
                    EXT4_CONFIG,
                )
                .expect("failed to mount ext4 root filesystem"),
            ),
        )
    };
}

fn normalized_root_name(path: &str) -> Option<&str> {
    let path = path.trim_start_matches('/');
    if path.is_empty() || path.contains('/') {
        None
    } else {
        Some(path)
    }
}

fn root_lookup_ino(fs: &mut KernelExt4Fs, path: &str) -> Option<(u32, InodeType)> {
    let name = normalized_root_name(path)?;
    let mut result = fs.lookup(EXT4_ROOT_INO, name).ok()?;
    let entry = result.entry();
    Some((entry.ino(), entry.inode_type()))
}

pub fn list_apps() {
    println!("/**** APPS ****");
    ROOT_FS.exclusive_session(|fs| {
        let mut reader = fs
            .0
            .read_dir(EXT4_ROOT_INO, 0)
            .expect("failed to iterate ext4 root directory");
        while let Some(entry) = reader.current() {
            let name = str::from_utf8(entry.name()).unwrap_or("<invalid>");
            if name != "." && name != ".." {
                println!("{}", name);
            }
            reader.step().expect("failed to advance ext4 dir iterator");
        }
    });
    println!("**************/")
}

bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Return (readable, writable).
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let name = normalized_root_name(name)?;
    let (readable, writable) = flags.read_write();
    let ino = ROOT_FS.exclusive_session(|fs| {
        let fs = &mut fs.0;
        if flags.contains(OpenFlags::CREATE) {
            if let Some((ino, inode_type)) = root_lookup_ino(fs, name) {
                if inode_type == InodeType::Directory {
                    return None;
                }
                fs.set_len(ino, 0).ok()?;
                Some(ino)
            } else {
                fs.create(EXT4_ROOT_INO, name, InodeType::RegularFile, 0o644)
                    .ok()
            }
        } else {
            let (ino, inode_type) = root_lookup_ino(fs, name)?;
            if inode_type == InodeType::Directory {
                return None;
            }
            if flags.contains(OpenFlags::TRUNC) {
                fs.set_len(ino, 0).ok()?;
            }
            Some(ino)
        }
    })?;
    Some(Arc::new(OSInode::new(readable, writable, ino)))
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = ROOT_FS.exclusive_session(|fs| {
                fs.0
                    .read_at(inner.ino, slice, inner.offset as u64)
                    .expect("ext4 read failed")
            });
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }

    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = ROOT_FS.exclusive_session(|fs| {
                fs.0
                    .write_at(inner.ino, slice, inner.offset as u64)
                    .expect("ext4 write failed")
            });
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }
}
