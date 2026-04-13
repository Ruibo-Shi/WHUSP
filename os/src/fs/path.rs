use super::ext4::FsNodeKind;
use super::mount::{
    MountId, aux_mount_id, has_aux_mount, is_aux_mount, primary_mount_id, resolve_primary_mount,
    with_mount,
};

// TODO: add support for two disks path resolution
pub(super) struct ResolvedFile {
    pub mount_id: MountId,
    pub ino: u32,
    pub kind: FsNodeKind,
}

pub(super) struct CreateTarget<'a> {
    pub mount_id: MountId,
    pub parent_ino: u32,
    pub leaf_name: &'a str,
}

pub(super) enum ResolvedOpen<'a> {
    Existing(ResolvedFile),
    Create(CreateTarget<'a>),
}

fn is_bare_name(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains('/')
}

fn resolve_on_mount<'a>(
    mount_id: MountId,
    relpath: &'a str,
    require_writable: bool,
    for_create: bool,
) -> Option<ResolvedOpen<'a>> {
    if is_aux_mount(mount_id) && (require_writable || for_create) {
        return None;
    }

    with_mount(mount_id, |mount| {
        if for_create {
            if let Some((ino, kind)) = mount.lookup_path(relpath) {
                Some(ResolvedOpen::Existing(ResolvedFile {
                    mount_id,
                    ino,
                    kind,
                }))
            } else {
                let (parent_ino, leaf_name) = mount.resolve_parent(relpath)?;
                Some(ResolvedOpen::Create(CreateTarget {
                    mount_id,
                    parent_ino,
                    leaf_name,
                }))
            }
        } else {
            let (ino, kind) = mount.lookup_path(relpath)?;
            Some(ResolvedOpen::Existing(ResolvedFile {
                mount_id,
                ino,
                kind,
            }))
        }
    })
    .flatten()
}

pub(super) fn resolve_open_target(
    path: &str,
    require_writable: bool,
    for_create: bool,
) -> Option<ResolvedOpen<'_>> {
    if path.starts_with('/') {
        let (mount_id, relpath) = resolve_primary_mount(path)?;
        return resolve_on_mount(mount_id, relpath, require_writable, for_create);
    }

    if is_bare_name(path) {
        if let Some(resolved) =
            resolve_on_mount(primary_mount_id(), path, require_writable, for_create)
        {
            return Some(resolved);
        }
        if !for_create && has_aux_mount() {
            return resolve_on_mount(aux_mount_id(), path, false, false);
        }
        return None;
    }

    None
}
