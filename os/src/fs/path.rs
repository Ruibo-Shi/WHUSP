use super::ext4::FsNodeKind;
use super::mount::{MountId, mount_exists, primary_mount_id, with_mount};

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

fn parse_prefixed_mount(component: &str) -> Option<MountId> {
    let suffix = component.strip_prefix('x')?;
    let index = suffix.parse::<usize>().ok()?;
    (index != 0).then_some(MountId(index))
}

fn resolve_absolute_mount(path: &str) -> Option<(MountId, &str)> {
    let relpath = path.strip_prefix('/')?;
    if relpath.is_empty() {
        return Some((primary_mount_id(), ""));
    }

    let (first_component, rest) = match relpath.split_once('/') {
        Some((first_component, rest)) => (first_component, Some(rest)),
        None => return Some((primary_mount_id(), relpath)),
    };

    let Some(mount_id) = parse_prefixed_mount(first_component) else {
        return Some((primary_mount_id(), relpath));
    };

    let mount_relpath = rest?.trim_start_matches('/');
    if mount_relpath.is_empty() || !mount_exists(mount_id) {
        return None;
    }
    Some((mount_id, mount_relpath))
}

fn resolve_on_mount<'a>(
    mount_id: MountId,
    relpath: &'a str,
    for_create: bool,
) -> Option<ResolvedOpen<'a>> {
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
    _require_writable: bool,
    for_create: bool,
) -> Option<ResolvedOpen<'_>> {
    if path.starts_with('/') {
        let (mount_id, relpath) = resolve_absolute_mount(path)?;
        return resolve_on_mount(mount_id, relpath, for_create);
    }

    if is_bare_name(path) {
        return resolve_on_mount(primary_mount_id(), path, for_create);
    }

    None
}
