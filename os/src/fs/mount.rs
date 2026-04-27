use super::ext4::Ext4Mount;
use super::path::WorkingDir;
use crate::drivers::block::BLOCK_DEVICES;
use crate::sync::UPIntrFreeCell;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::*;
use log::{info, warn};
use lwext4_rust::ffi::EXT4_ROOT_INO;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MountId(pub(crate) usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DynamicMount {
    target_mount_id: MountId,
    target_ino: u32,
    source_mount_id: MountId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MountError {
    SourceMissing,
    TargetBusy,
    TargetNotMounted,
    StaticRoot,
}

lazy_static! {
    static ref MOUNTS: Vec<UPIntrFreeCell<Option<Ext4Mount>>> = BLOCK_DEVICES
        .iter()
        .map(|_| unsafe { UPIntrFreeCell::new(None) })
        .collect();
    static ref MOUNTS_INITIALIZED: UPIntrFreeCell<bool> = unsafe { UPIntrFreeCell::new(false) };
    static ref DYNAMIC_MOUNTS: UPIntrFreeCell<Vec<DynamicMount>> =
        unsafe { UPIntrFreeCell::new(Vec::new()) };
}

pub fn init_mounts() {
    // TODO: a little bit too much ...
    let already_initialized = MOUNTS_INITIALIZED.exclusive_session(|initialized| {
        if *initialized {
            true
        } else {
            *initialized = true;
            false
        }
    });
    if already_initialized {
        return;
    }

    for (index, device) in BLOCK_DEVICES.iter().enumerate() {
        let mount = if index == 0 {
            Some(Ext4Mount::open(device.clone()).expect("failed to mount primary ext4 filesystem"))
        } else {
            match Ext4Mount::open(device.clone()) {
                Ok(mount) => Some(mount),
                Err(err) => {
                    warn!("failed to mount filesystem on BLOCK_DEVICES[{index}]: {err:?}");
                    None
                }
            }
        };
        MOUNTS[index].exclusive_session(|slot| *slot = mount);
    }
}

pub(super) fn with_mount<V>(mount_id: MountId, f: impl FnOnce(&mut Ext4Mount) -> V) -> Option<V> {
    MOUNTS
        .get(mount_id.0)
        .and_then(|slot| slot.exclusive_session(|mount| mount.as_mut().map(f)))
}

pub(super) fn mount_exists(mount_id: MountId) -> bool {
    MOUNTS
        .get(mount_id.0)
        .is_some_and(|slot| slot.exclusive_session(|mount| mount.is_some()))
}

pub(super) fn mounted_root_for(mount_id: MountId, ino: u32) -> Option<MountId> {
    DYNAMIC_MOUNTS.exclusive_session(|mounts| {
        mounts
            .iter()
            .rev()
            .find(|mount| mount.target_mount_id == mount_id && mount.target_ino == ino)
            .map(|mount| mount.source_mount_id)
    })
}

// TODO: maybe we could skip this function
pub(super) fn primary_mount_id() -> MountId {
    MountId(0)
}

pub(crate) fn mount_block_device_at(
    target: WorkingDir,
    device_index: usize,
) -> Result<(), MountError> {
    let source_mount_id = MountId(device_index);
    if !mount_exists(source_mount_id) {
        return Err(MountError::SourceMissing);
    }
    if target.ino() == EXT4_ROOT_INO {
        return Err(MountError::StaticRoot);
    }

    DYNAMIC_MOUNTS.exclusive_session(|mounts| {
        if mounts.iter().any(|mount| {
            mount.target_mount_id == target.mount_id() && mount.target_ino == target.ino()
        }) {
            return Err(MountError::TargetBusy);
        }
        mounts.push(DynamicMount {
            target_mount_id: target.mount_id(),
            target_ino: target.ino(),
            source_mount_id,
        });
        Ok(())
    })
}

pub(crate) fn unmount_at(target: WorkingDir) -> Result<(), MountError> {
    DYNAMIC_MOUNTS.exclusive_session(|mounts| {
        if let Some(index) = mounts.iter().rposition(|mount| {
            mount.target_mount_id == target.mount_id() && mount.target_ino == target.ino()
        }) {
            mounts.remove(index);
            Ok(())
        } else {
            Err(MountError::TargetNotMounted)
        }
    })
}

pub fn mount_status_log() {
    info!("filesystem mounted from BLOCK_DEVICES[0] at /");
    for index in 1..MOUNTS.len() {
        if mount_exists(MountId(index)) {
            info!("filesystem mounted from BLOCK_DEVICES[{index}] at /x{index}");
        } else {
            info!("filesystem on BLOCK_DEVICES[{index}] is unavailable at /x{index}",);
        }
    }
}

pub fn list_root_apps() -> Vec<String> {
    with_mount(primary_mount_id(), |mount| mount.list_root_names()).unwrap_or_default()
}
