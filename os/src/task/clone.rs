use bitflags::bitflags;

bitflags! {
    /// Linux `clone(2)` flags. Low 8 bits of the raw `flags` argument are the
    /// exit_signal (e.g. `SIGCHLD = 17`) and are extracted before constructing
    /// this bitflags value.
    pub struct CloneFlags: u32 {
        const CLONE_VM              = 1 << 8;
        const CLONE_FS              = 1 << 9;
        const CLONE_FILES           = 1 << 10;
        const CLONE_SIGHAND         = 1 << 11;
        const CLONE_PTRACE          = 1 << 13;
        const CLONE_VFORK           = 1 << 14;
        const CLONE_PARENT          = 1 << 15;
        const CLONE_THREAD          = 1 << 16;
        const CLONE_NEWNS           = 1 << 17;
        const CLONE_SYSVSEM         = 1 << 18;
        const CLONE_SETTLS          = 1 << 19;
        const CLONE_PARENT_SETTID   = 1 << 20;
        const CLONE_CHILD_CLEARTID  = 1 << 21;
        const CLONE_DETACHED        = 1 << 22;
        const CLONE_UNTRACED        = 1 << 23;
        const CLONE_CHILD_SETTID    = 1 << 24;
    }
}
