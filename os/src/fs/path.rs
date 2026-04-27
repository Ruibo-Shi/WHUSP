use super::ext4::FsNodeKind;
use super::mount::{MountId, mount_exists, mounted_root_for, primary_mount_id, with_mount};
use lwext4_rust::ffi::EXT4_ROOT_INO;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkingDir {
    mount_id: MountId,
    ino: u32,
}

impl WorkingDir {
    pub(crate) fn root() -> Self {
        Self {
            mount_id: primary_mount_id(),
            ino: EXT4_ROOT_INO,
        }
    }

    pub(crate) fn new(mount_id: MountId, ino: u32) -> Self {
        Self { mount_id, ino }
    }

    pub(crate) fn mount_id(self) -> MountId {
        self.mount_id
    }

    pub(crate) fn ino(self) -> u32 {
        self.ino
    }
}

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

pub(super) struct ParentTarget<'a> {
    pub mount_id: MountId,
    pub parent_ino: u32,
    pub leaf_name: &'a str,
}

pub(super) enum ResolvedOpen<'a> {
    Existing(ResolvedFile),
    Create(CreateTarget<'a>),
}

#[derive(Clone, Copy, Debug)]
struct PathCursor {
    mount_id: MountId,
    ino: u32,
    kind: FsNodeKind,
}

impl PathCursor {
    fn root() -> Self {
        Self {
            mount_id: primary_mount_id(),
            ino: EXT4_ROOT_INO,
            kind: FsNodeKind::Directory,
        }
    }

    fn from_working_dir(cwd: WorkingDir) -> Self {
        Self {
            mount_id: cwd.mount_id(),
            ino: cwd.ino(),
            kind: FsNodeKind::Directory,
        }
    }

    fn as_resolved_file(self) -> ResolvedFile {
        ResolvedFile {
            mount_id: self.mount_id,
            ino: self.ino,
            kind: self.kind,
        }
    }

    fn is_primary_root(self) -> bool {
        self.mount_id == primary_mount_id() && self.ino == EXT4_ROOT_INO
    }

    fn is_mount_root(self) -> bool {
        self.ino == EXT4_ROOT_INO
    }
}

fn parse_prefixed_mount(component: &str) -> Option<MountId> {
    let suffix = component.strip_prefix('x')?;
    let index = suffix.parse::<usize>().ok()?;
    (index != 0).then_some(MountId(index))
}

fn follow_mounted_root(cursor: PathCursor) -> PathCursor {
    if cursor.kind != FsNodeKind::Directory {
        return cursor;
    }
    if let Some(mount_id) = mounted_root_for(cursor.mount_id, cursor.ino) {
        return PathCursor {
            mount_id,
            ino: EXT4_ROOT_INO,
            kind: FsNodeKind::Directory,
        };
    }
    cursor
}

fn lookup_child_raw(cursor: PathCursor, component: &str) -> Option<PathCursor> {
    if cursor.kind != FsNodeKind::Directory {
        return None;
    }

    // CONTEXT: `/xN` is this kernel's virtual mount-prefix namespace, not a real
    // directory entry on the primary EXT4 filesystem.
    if cursor.is_primary_root() {
        if let Some(mount_id) = parse_prefixed_mount(component) {
            if mount_exists(mount_id) {
                return Some(PathCursor {
                    mount_id,
                    ino: EXT4_ROOT_INO,
                    kind: FsNodeKind::Directory,
                });
            }
            return None;
        }
    }

    let (ino, kind) = with_mount(cursor.mount_id, |mount| {
        mount.lookup_component_from(cursor.ino, component)
    })??;
    Some(PathCursor {
        mount_id: cursor.mount_id,
        ino,
        kind,
    })
}

fn lookup_parent(cursor: PathCursor) -> Option<PathCursor> {
    if cursor.is_mount_root() {
        // UNFINISHED: Dynamic mounts do not remember the covered directory's
        // parent, so `..` from a mounted root currently falls back to `/`.
        if cursor.mount_id == primary_mount_id() {
            return Some(PathCursor::root());
        }
        return Some(PathCursor::root());
    }
    lookup_child_raw(cursor, "..")
}

fn start_cursor(cwd: Option<WorkingDir>, path: &str) -> PathCursor {
    if path.starts_with('/') {
        PathCursor::root()
    } else if let Some(cwd) = cwd {
        PathCursor::from_working_dir(cwd)
    } else {
        PathCursor::root()
    }
}

fn resolve_path_inner(
    cwd: Option<WorkingDir>,
    path: &str,
    follow_final_mount: bool,
) -> Option<PathCursor> {
    let mut cursor = start_cursor(cwd, path);
    let mut components = path
        .split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .peekable();
    if follow_final_mount && components.peek().is_none() {
        cursor = follow_mounted_root(cursor);
    }
    while let Some(component) = components.next() {
        if component == ".." {
            cursor = lookup_parent(cursor)?;
        } else {
            cursor = lookup_child_raw(cursor, component)?;
        }
        if follow_final_mount || components.peek().is_some() {
            cursor = follow_mounted_root(cursor);
        }
    }
    Some(cursor)
}

fn resolve_path(cwd: Option<WorkingDir>, path: &str) -> Option<PathCursor> {
    resolve_path_inner(cwd, path, true)
}

pub(super) fn resolve_mount_target(cwd: Option<WorkingDir>, path: &str) -> Option<ResolvedFile> {
    Some(resolve_path_inner(cwd, path, false)?.as_resolved_file())
}

fn split_parent_path(path: &str) -> Option<(&str, &str)> {
    if path.is_empty() {
        return None;
    }
    let (parent_path, leaf_name) = match path.rsplit_once('/') {
        Some((parent_path, leaf_name)) => (parent_path, leaf_name),
        None => ("", path),
    };
    if leaf_name.is_empty() || leaf_name == "." || leaf_name == ".." {
        return None;
    }
    Some((parent_path, leaf_name))
}

fn parent_path_for_lookup<'a>(path: &str, parent_path: &'a str) -> &'a str {
    if path.starts_with('/') && parent_path.is_empty() {
        "/"
    } else {
        parent_path
    }
}

pub(super) fn resolve_open_target(
    cwd: Option<WorkingDir>,
    path: &str,
    _require_writable: bool,
    for_create: bool,
) -> Option<ResolvedOpen<'_>> {
    if let Some(existing) = resolve_path(cwd, path) {
        return Some(ResolvedOpen::Existing(existing.as_resolved_file()));
    }

    if !for_create {
        return None;
    }
    let (parent_path, leaf_name) = split_parent_path(path)?;
    let parent_path = parent_path_for_lookup(path, parent_path);
    let parent = resolve_path(cwd, parent_path)?;
    if parent.kind != FsNodeKind::Directory {
        return None;
    }
    Some(ResolvedOpen::Create(CreateTarget {
        mount_id: parent.mount_id,
        parent_ino: parent.ino,
        leaf_name,
    }))
}

pub(super) fn resolve_parent_target(
    cwd: Option<WorkingDir>,
    path: &str,
) -> Option<ParentTarget<'_>> {
    let (parent_path, leaf_name) = split_parent_path(path)?;
    let parent_path = parent_path_for_lookup(path, parent_path);
    let parent = resolve_path(cwd, parent_path)?;
    if parent.kind != FsNodeKind::Directory {
        return None;
    }
    Some(ParentTarget {
        mount_id: parent.mount_id,
        parent_ino: parent.ino,
        leaf_name,
    })
}

pub(crate) fn normalize_path(cwd_path: &str, path: &str) -> Option<alloc::string::String> {
    let mut segments = alloc::vec::Vec::new();
    if path.starts_with('/') {
        for segment in path.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                segments.pop();
            } else {
                segments.push(segment);
            }
        }
    } else {
        for segment in cwd_path.split('/') {
            if segment.is_empty() {
                continue;
            }
            segments.push(segment);
        }
        for segment in path.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                segments.pop();
            } else {
                segments.push(segment);
            }
        }
    }

    if segments.is_empty() {
        Some("/".into())
    } else {
        Some(alloc::format!("/{}", segments.join("/")))
    }
}
