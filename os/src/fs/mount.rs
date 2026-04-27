use super::ext4::{Ext4Mount, FsNodeKind};
use super::path::WorkingDir;
use crate::drivers::block::BLOCK_DEVICES;
use crate::sync::UPIntrFreeCell;
use alloc::vec::Vec;
use alloc::{format, string::String};
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
    InvalidFilesystem,
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

    let primary_device = BLOCK_DEVICES
        .first()
        .expect("DTB is missing a block device")
        .clone();
    let primary_mount =
        Ext4Mount::open(primary_device).expect("failed to mount primary ext4 filesystem");
    MOUNTS[0].exclusive_session(|slot| *slot = Some(primary_mount));

    mount_extra_block_devices();
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

fn ensure_mount_open(mount_id: MountId) -> Result<(), MountError> {
    let Some(slot) = MOUNTS.get(mount_id.0) else {
        return Err(MountError::SourceMissing);
    };
    if slot.exclusive_session(|mount| mount.is_some()) {
        return Ok(());
    }

    let device = BLOCK_DEVICES
        .get(mount_id.0)
        .ok_or(MountError::SourceMissing)?
        .clone();
    let mount = Ext4Mount::open(device).map_err(|err| {
        warn!(
            "failed to open ext4 filesystem on BLOCK_DEVICES[{}]: {:?}",
            mount_id.0, err
        );
        MountError::InvalidFilesystem
    })?;
    slot.exclusive_session(|slot| {
        if slot.is_none() {
            *slot = Some(mount);
        }
    });
    Ok(())
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
    if target.ino() == EXT4_ROOT_INO {
        return Err(MountError::StaticRoot);
    }

    let target_is_busy = DYNAMIC_MOUNTS.exclusive_session(|mounts| {
        mounts.iter().any(|mount| {
            mount.target_mount_id == target.mount_id() && mount.target_ino == target.ino()
        })
    });
    if target_is_busy {
        return Err(MountError::TargetBusy);
    }

    ensure_mount_open(source_mount_id)?;

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

fn ensure_extra_mount_target(index: usize) -> Option<WorkingDir> {
    let name = format!("x{index}");
    with_mount(primary_mount_id(), |mount| {
        if let Some((ino, kind)) = mount.lookup_component_from(EXT4_ROOT_INO, &name) {
            if kind == FsNodeKind::Directory {
                return Some(WorkingDir::new(primary_mount_id(), ino));
            }
            warn!("cannot auto-mount BLOCK_DEVICES[{index}]: /{name} is not a directory");
            return None;
        }

        mount
            .create_dir(EXT4_ROOT_INO, &name, 0o755)
            .map(|ino| WorkingDir::new(primary_mount_id(), ino))
            .or_else(|| {
                warn!("cannot create /{name} for BLOCK_DEVICES[{index}] auto-mount");
                None
            })
    })
    .flatten()
}

fn source_has_dynamic_mount(source_mount_id: MountId) -> bool {
    DYNAMIC_MOUNTS.exclusive_session(|mounts| {
        mounts
            .iter()
            .any(|mount| mount.source_mount_id == source_mount_id)
    })
}

fn mount_extra_block_devices() {
    for index in 1..BLOCK_DEVICES.len() {
        let Some(target) = ensure_extra_mount_target(index) else {
            continue;
        };
        match mount_block_device_at(target, index) {
            Ok(()) => info!("auto-mounted BLOCK_DEVICES[{index}] at /x{index}"),
            Err(MountError::InvalidFilesystem) => {
                warn!("BLOCK_DEVICES[{index}] is not an ext4 filesystem; leaving /x{index} empty")
            }
            Err(err) => warn!("failed to auto-mount BLOCK_DEVICES[{index}] at /x{index}: {err:?}"),
        }
    }
}

pub fn mount_status_log() {
    info!("filesystem mounted from BLOCK_DEVICES[0] at /");
    for index in 1..MOUNTS.len() {
        if source_has_dynamic_mount(MountId(index)) {
            info!("filesystem mounted from BLOCK_DEVICES[{index}] at /x{index}");
        } else if mount_exists(MountId(index)) {
            info!("filesystem on BLOCK_DEVICES[{index}] is open but not mounted");
        } else {
            info!("filesystem on BLOCK_DEVICES[{index}] is not mounted");
        }
    }
}

pub fn list_root_apps() -> Vec<String> {
    with_mount(primary_mount_id(), |mount| mount.list_root_names()).unwrap_or_default()
}
