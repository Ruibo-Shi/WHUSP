# OSKernel2026 开发任务清单

## P0 — 提交闭环（contest submission blockers）

- [x] 重写根目录 `Makefile`，让 `make all` 成为正式提交入口
- [x] 让根目录 `make all` 产出 `kernel-rv`
- [ ] 让根目录 `make all` 产出 `kernel-la`
- [x] 清理提交链路对隐藏目录 `.cargo` 的依赖（仓库已无 `.cargo/`）
- [x] 把远程 Cargo 依赖改成离线可构建方案（`vendor/crates` + `vendor/config.toml`）
- [x] 在本地验证无网络构建仍然可用（`CARGO_NET_OFFLINE=true make all`）
- [x] 修改 `initproc`，支持比赛模式启动（当前内核直接加载 `/musl/busybox sh`）
- [ ] 新增 submit runner 用户程序
- [ ] 让 submit runner 按固定顺序串行执行测试脚本
- [ ] 输出精确的 `#### OS COMP TEST GROUP START xxxxx ####`
- [ ] 输出精确的 `#### OS COMP TEST GROUP END xxxxx ####`
- [ ] 在所有测试组结束后主动关机

## P1 — 启动与设备基线（基本完成，保留为记录）

- [x] 修正 `os/src/boards/qemu.rs`，去掉对 GPU 的硬依赖（已改为 `Option<IrqDevice>`）
- [x] 修正 `os/src/boards/qemu.rs`，去掉对键盘的硬依赖
- [x] 修正 `os/src/boards/qemu.rs`，去掉对鼠标的硬依赖
- [x] 用官方评测风格的无头 QEMU 命令验证内核可以启动（`ci-riscv-smoke.yml` 已覆盖）
- [x] 升级块设备发现逻辑，支持识别多个 virtio block 设备（`BLOCK_DEVICE_CAPACITY=8`，按 base 排序）
- [x] 明确区分评测盘 `x0` 和可选辅助块设备 `x1`
- [x] 设计并实现动态挂载路径（`x0` → `/`，额外盘 lazy-open 后覆盖真实目录 `/x1`、`/x2`）
- [x] 接入评测 EXT4 测试盘的只读访问

## P2 — basic-musl syscall 补齐（跑通 `/musl/basic_testcode.sh` 前置）

- [ ] 补齐 `basic-musl` 需要的 syscall（父任务）
  - [x] 补齐目录遍历与 `getdents64`
  - [x] `openat(56)` 基础版本
  - [x] `mkdirat(34)` / `unlinkat(35)` / `chdir(49)` / `getcwd(17)`
  - [ ] 升级 `openat` 相关语义（flags / mode / O_CREAT / O_DIRECTORY 完整行为）
  - [x] 升级 `execve` 的 `argv/envp` 传递（当前 `sys_exec` 只接 2 参数，无 envp）
  - [ ] 升级 `wait4/waitpid` 相关语义（options / rusage）
  - [ ] 补齐 `stat/fstat/newfstatat` 相关语义
  - [ ] 补齐 `mmap/munmap/brk` 相关语义
- [ ] 根据 `/musl/basic/run-all.sh` 输出继续补齐（2026-04-27）
  - [x] 当前已跑通（至少 basic 用例已观察通过）：`brk` / `chdir` / `clone` / `close` / `dup2` / `execve` / `exit` / `fork` / `fstat` / `getcwd` / `getdents64` / `getpid` / `mkdirat` / `mmap` / `munmap` / `openat` / `pipe` / `read` / `unlinkat` / `wait4(wait/waitpid)` / `write` / `yield`
  - [ ] `dup` 语义仍异常（`test_dup` 触发 assert）
  - [x] `getppid(173)` 已补齐（`test_getppid` 输出 success）
  - [x] `gettimeofday(169)` 已补齐（`test_gettimeofday` 输出 success）
  - [ ] `sleep` 相关语义仍不兼容（`test_sleep` 触发 assert）
  - [ ] `times(153)` 未补齐（`test_times` 触发 assert）
  - [ ] `uname(160)` 仍不兼容（`test_uname` 触发 assert）
  - [ ] `mount(40)` / `umount2(39)` 竞赛测试语义仍需完善（当前只支持 whole-disk ext4；`/dev/vda2` 分区和 `vfat` 仍未支持，FAT 库候选见 P2.6.5）
- [ ] 让 `/musl/basic_testcode.sh` 可以完整跑通
  - [ ] 非 syscall 问题：`/musl/basic_testcode.sh` 无 shebang，需用 `./busybox sh ./basic_testcode.sh` 执行

### syscall ABI 合规性审计（参考 `reference-project/RocketOS`、`oskernel_neverdown`、`NighthawkOS`、`RustOsWhu`；每条独立一轮，动手前对照 `man 2` + 参考实现）

- [x] 统一 `SYSCALL_OPENAT = 56` 命名：user 侧 `user/src/syscall.rs:9` 写作 `SYSCALL_OPEN`，kernel 侧 `os/src/syscall/mod.rs:9` 写作 `SYSCALL_OPENAT`，同号异名；语义已经是 openat，只是命名需要对齐
- [x] `sys_waitpid`(260) 升级为 `sys_wait4(pid, wstatus, options, rusage)`（基础路径已接入，后续继续补 Linux 细节）
- [x] 实现 `sys_exit_group`(94)（当前为单线程兼容实现，后续补完整线程组语义）
- [ ] 修正 `sys_kill`(129) 信号参数类型：`os/src/syscall/process.rs:106` 用 `SignalFlags::from_bits(signal)` 把信号当 bitflags，但 Linux 信号号是整数（SIGKILL=9、SIGTERM=15 不是位标志），应直接按 signum 分发
- [ ] errno support?

## P2.5 — cwd in pcb 收尾

- [ ] cwd in pcb（父任务）
  - [x] widen syscall arg forwarding to 6 args for Linux pathname syscalls
  - [x] add pcb `cwd_path` string alongside `WorkingDir`
  - [x] allow directory fd open and dirfd base extraction
  - [x] implement `chdir(49)`
  - [x] implement `getcwd(17)`
  - [x] upgrade syscall 56 to real `openat`
  - [x] implement `mkdirat(34)`
  - [x] implement `unlinkat(35)` for file removal
  - [ ] implement `fchdir(50)`
  - [ ] implement `newfstatat` / `fstatat`（与 P2 的 stat 家族合并推进）
  - [ ] implement `readlinkat`
  - [ ] implement `faccessat`
  - [ ] implement `renameat2`
  - [ ] implement `chroot`
  - [ ] implement `openat2`
  - [ ] support `..` in relative path resolution（部分完成，`os/src/fs/path.rs:209` 仍有 TODO）
  - [ ] support symlink traversal / nofollow semantics
  - [ ] make mount/umount target path respect cwd-relative resolution

## P2.6 — VFS 稳健化路线图（先稳住并发，再补 Linux 语义）

- [ ] 阶段 0：冻结当前事实与回归用例
  - [ ] 记录当前文件系统调用链：`sys_exec/open/read -> open_file_at -> path.rs -> with_mount -> Ext4Mount -> VirtIOBlock`
  - [ ] 记录当前崩溃复现：BusyBox pipeline 并发 `exec` 会触发 `UPIntrFreeCell` 的 `already borrowed`
  - [ ] 加一个最小手工验收命令：`/musl/busybox ls /musl/basic | /musl/busybox grep gettimeofday` 不应 panic
  - [ ] 加一个基础正例验收命令：`/musl/basic/pipe` 仍输出 `Write to pipe successfully.`
  - [ ] 明确不变量：`UPIntrFreeCell` 只允许用于不会阻塞、不会 `schedule()` 的短临界区
- [ ] 阶段 1：修掉 mount/EXT4 跨调度借用 panic
  - [ ] 新增可睡眠的内核互斥原语（例如 `SleepMutex<T>` / `BlockingMutex<T>`），用于可能在持锁期间等待 I/O 的对象
  - [ ] 将 `MOUNTS: Vec<UPIntrFreeCell<Option<Ext4Mount>>>` 迁移为可睡眠锁保护的 mount slot
  - [ ] 保持 `DYNAMIC_MOUNTS` 仍使用短临界区锁；不要让动态挂载表操作进入块设备 I/O
  - [ ] 改造 `with_mount()`：同一 mount 已被其他任务使用时应阻塞/让出，而不是 `RefCell::borrow_mut()` panic
  - [ ] 验证 pipeline 复现命令、`/musl/basic/pipe`、`/musl/basic/gettimeofday`、`make run-rv-dev`
  - [ ] 用 `timeout` 包住验证命令，确认没有死锁或永久等待
- [ ] 阶段 2：拆出最小 VFS 对象层，但保持现有行为
  - [ ] 新建 `os/src/fs/vfs/`，先只承载类型与转发逻辑，不改用户可见语义
  - [ ] 定义 `VfsNodeId { mount_id, ino }`，替代散落的 `(MountId, ino)` 参数传递
  - [ ] 定义 `VfsPath { node: VfsNodeId, kind }`，作为路径解析结果
  - [ ] 定义 `VfsFile { node, offset, readable, writable, status_flags }`，替代 `OSInodeInner` 承担普通文件 fd 状态
  - [ ] 保留现有 `File` trait 作为 fd table 对外接口；pipe/stdin/stdout 暂不迁入 VFS node 模型
  - [ ] 将 `open_file_at/stat_at/lookup_dir_at` 改成调用 VFS 层，再由 VFS 调 ext4 后端
  - [ ] 每次迁移后跑 `make all`、`make run-rv-dev`、contest-style `/musl/basic/{open,read,getdents,fstat}`
- [ ] 阶段 3：正规化路径解析与 mount crossing
  - [ ] 把 `path.rs` 的 `PathCursor` 迁到 VFS 层，统一返回 `VfsPath`
  - [ ] 让 mount crossing 明确处理“覆盖目录”和“被挂载文件系统根目录”的关系
  - [ ] 记录 mounted root 的父目录信息，修复当前 `..` 从 mounted root 回 `/` 的临时行为
  - [ ] 区分普通 lookup、mount target lookup、create parent lookup，避免 `mount` 目标被错误 follow
  - [ ] 增加 symlink 预留接口：先返回 `ELOOP/ENOSYS` 或保持未实现标记，不混入真实 symlink 语义
  - [ ] 验证绝对路径、相对路径、`..`、`/x1`、mount target、unmount 后路径行为
- [ ] 阶段 4：把 EXT4 后端收敛成 VFS backend trait
  - [ ] 定义 `FileSystemBackend` trait：`lookup/read/write/stat/create/unlink/readdir`
  - [ ] 让 `Ext4Mount` 实现 backend trait；lwext4 细节只留在 `ext4.rs`
  - [ ] 将 backend 锁放在 mount 实例内部，VFS 层只拿 trait object / mount handle
  - [ ] 为后续 `tmpfs/devfs/procfs` 预留 backend 注册点，但本阶段只注册 ext4
  - [ ] 验证现有 `File` trait 对 fd table 的行为不变
- [ ] 阶段 5：补 Linux VFS 关键语义
  - [ ] 完善 `openat`：`O_CREAT/O_EXCL/O_TRUNC/O_DIRECTORY/O_NOFOLLOW/O_APPEND/O_NONBLOCK`
  - [ ] 完善 `newfstatat/fstat/lstat`：目录、普通文件、pipe、stdio 的 `mode/dev/ino/nlink/size`
  - [ ] 完善 `getdents64`：目录 offset 稳定性、buffer 边界、跨 mount 目录读取
  - [ ] 完善 `renameat2/linkat/symlinkat/readlinkat/faccessat/fchdir`
  - [ ] 完善 `mount/umount2`：busy target、mounted root、相对路径、错误码；分区和非 ext4 另列后续任务
  - [ ] 所有不完整语义必须用 `// UNFINISHED:` 标出具体 Linux 缺口
- [ ] 阶段 6：加入缓存与性能，不提前优化
  - [ ] 在语义稳定后再加 inode cache；cache key 使用 `(mount_id, ino)`
  - [ ] 加正向 dentry cache；负向 cache 等 rename/unlink 语义稳定后再考虑
  - [ ] 加简单 page/block cache，先服务 ELF 加载、顺序读、`getdents64`
  - [ ] 为 cache 加失效路径：`create/unlink/rename/truncate/write`
  - [ ] 对比 `huge_write`、BusyBox pipeline、重复 exec BusyBox 的性能与正确性
- [ ] 阶段 7：验收门槛
  - [ ] `make fmt`
  - [ ] `make all`
  - [ ] `CARGO_NET_OFFLINE=true make all`
  - [ ] `make run-rv-dev`
  - [ ] `make run-rv` 下执行 `/musl/basic_testcode.sh` 或等价 BusyBox shell 包装
  - [ ] pipeline 复现不 panic，重复运行 5 次不死锁
  - [ ] `basic-musl` 文件系统相关用例全部通过：`open/openat/read/write/getdents/fstat/mkdir/unlink/chdir/getcwd/mount/umount/pipe/execve`

## P2.6.5 — 可选 FAT/VFAT 支持路线图（用于 mount basic 测例兼容）

- [ ] 采用 `starry-fatfs` 作为首选 FAT 库：它是 `rust-fatfs` 的 Starry-OS fork，crates.io 当前包名为 `starry-fatfs`，导入 crate 名仍按 `fatfs` 使用；计划 manifest 写法为 `fatfs = { package = "starry-fatfs", version = "0.4.1-preview.2", default-features = false, features = ["alloc", "lfn", "unicode"] }`
- [ ] vendoring 前复核离线构建：把 `starry-fatfs` 及其依赖纳入 `vendor/crates`，并确认 `CARGO_NET_OFFLINE=true make all` 不访问网络
- [ ] 为 WHUSP 块设备实现 `fatfs::Read` / `fatfs::Write` / `fatfs::Seek` 适配层，先只面向 512-byte sector 的 VirtIO block 设备
- [ ] 在 `os/src/fs` 新增 FAT mount wrapper，把 `fatfs::FileSystem` 的 root dir、普通文件、目录遍历、create/remove/rename 基础能力映射到当前文件系统接口
- [ ] 泛化当前 mount 表：从 `MountId -> Ext4Mount` 演进到可同时承载 `Ext4Mount` 与 `FatMount` 的枚举或 trait object，保持现有 ext4 root 行为不变
- [ ] 在 `sys_mount` 中接受 `fstype == "vfat"` / `"fat32"`，并继续拒绝未实现 flags/data；`umount2` 复用现有动态卸载路径
- [ ] 先补 `/dev/vda2` 这类分区源解析，否则 basic mount 测例默认参数仍无法定位 FAT 分区
- [ ] FAT 语义边界：首轮不承诺 symlink、Unix owner/mode、hard link、完整时间戳和大小写规则；相关缺口用 `// UNFINISHED:` 标明
- [ ] 验证顺序：先用 FAT32 镜像做内核内只读 lookup/read，再跑 create/write/read/remove，最后跑 `/musl/basic/mount` 和 `/musl/basic/umount`

## P2.7 — syscall 层瘦身路线图（让 syscall 只做 ABI adapter）

- [ ] 阶段 0：冻结边界规则
  - [ ] 记录目标调用形态：`trap -> syscall dispatcher -> syscall adapter -> kernel subsystem`
  - [ ] 明确 syscall adapter 只允许做：参数取值、用户指针复制、flag/errno 转换、fd 引用获取、调用子系统
  - [ ] 明确 syscall adapter 不应长期承载：ELF/shebang 装载策略、VFS 路径遍历、底层文件系统细节、调度策略、缓存策略
  - [ ] 给 `os/src/syscall.rs` 加审计注释：新增 Linux syscall 时常量名必须对齐 `__NR_*`
  - [ ] 全量 grep 当前宽逻辑：`process.rs` 的 `execve` 装载、`fs/user_ptr.rs`、`fs/fd.rs`、`fs/path.rs`
- [ ] 阶段 1：先把通用 uaccess 从 fs syscall 中拆出
  - [ ] 新建 `os/src/mm/uaccess.rs` 或 `os/src/uaccess.rs`
  - [ ] 迁移 `translated_byte_buffer_checked/read_user_value/write_user_value/read_user_usize`
  - [ ] 保持旧调用点行为不变，只调整 import 路径
  - [ ] 给读/写权限检查保留 `UserBufferAccess::{Read, Write}`
  - [ ] 验证 `read/write/readv/writev/pipe2/gettimeofday/newfstatat` 不回退
- [ ] 阶段 2：拆出 exec 装载层
  - [ ] 新建 `os/src/loader/` 或扩展 `os/src/task/exec.rs`
  - [ ] 把 `ELF_MAGIC`、shebang 解析、递归限制、解释器 argv 重写迁出 `syscall/process.rs`
  - [ ] 将入口整理成 `do_execve(path, argv, envp) -> SysResult`
  - [ ] syscall 层只保留 `copy path/argv/envp from user -> do_execve`
  - [ ] 保留 BusyBox fallback，但用 `// CONTEXT:` 标清它是 contest test-disk 兼容策略
  - [ ] 验证 ELF 直接执行、脚本执行、`/musl/busybox sh`、无 shebang 脚本的错误路径
- [ ] 阶段 3：收敛 fd table 操作
  - [ ] 在 `task` 或 `fs/fd_table.rs` 提供 `fd_get/fd_alloc/fd_install/fd_close/fd_dup/fd_set_flags`
  - [ ] syscall 层不再直接遍历 `process.inner.fd_table`
  - [ ] 保持 `FdTableEntry` 承载 fd flags 与 status flags
  - [ ] 将 `dup/dup3/fcntl/pipe2/openat/close` 改为调用统一 fd table API
  - [ ] 验证 `dup`、`dup3(O_CLOEXEC)`、`fcntl(F_GETFD/F_SETFD/F_GETFL/F_SETFL)`、`pipe2`
- [ ] 阶段 4：把路径 syscall 变成 VFS adapter
  - [ ] 等 P2.6 的最小 VFS 对象层落地后再开始本阶段
  - [ ] `sys_openat/chdir/mkdirat/unlinkat/getdents64/newfstatat` 只做 ABI 参数处理
  - [ ] 路径基准目录选择统一下沉到 `vfs_at(dirfd, path)` 一类函数
  - [ ] syscall 层不再直接调用 `open_file_at/stat_at/lookup_dir_at`
  - [ ] 验证 cwd、dirfd、绝对路径、相对路径、mount crossing、错误码
- [ ] 阶段 5：整理 UAPI 类型和 syscall 命名
  - [ ] 将 Linux ABI 结构体集中放在 `os/src/syscall/uapi.rs` 或按域拆分 `syscall/*/uapi.rs`
  - [ ] 内核 syscall 常量名全部对齐 Linux `__NR_*`：如 `GETTIMEOFDAY/EXECVE/SCHED_YIELD/NEWFSTATAT`
  - [ ] 仓库私有 syscall 必须集中在 private range，并注释“不属于 Linux ABI”
  - [ ] 用户库便利 API 可以保留旧名，但底层 syscall 号常量必须用标准名
  - [ ] 每个语义不完整 syscall 保留精确 `// UNFINISHED:`，兼容策略保留精确 `// CONTEXT:`
- [ ] 阶段 6：建立防回退检查
  - [ ] 加一个 `rg` 检查脚本：禁止新增 `SYSCALL_GET_TIME/SYSCALL_EXEC/SYSCALL_SLEEP/SYSCALL_YIELD`
  - [ ] 加一个 `rg` 检查脚本：禁止在 `syscall/` 新增大块 ELF/shebang/VFS backend 逻辑
  - [ ] `make fmt`
  - [ ] `make all`
  - [ ] `make run-rv-dev`
  - [ ] `make run-rv` 下抽测 `/musl/basic/gettimeofday`、`/musl/basic/pipe`、BusyBox shell、pipeline 复现命令

## P3 — 扩展 libc 与动态链接

- [ ] 推进 `busybox` 需要的 shell / pipe / 重定向语义
- [ ] 推进 `lua` 所需的文件与执行环境兼容性
- [ ] 推进 `libctest-musl` 所需的动态链接与共享库运行时
- [ ] 补齐 `/lib/ld-musl-riscv64.so.1` 路径支持
- [ ] 推进 glibc 变体运行
- [ ] 补齐 `/lib/ld-linux-riscv64-lp64d.so.1` 路径支持
- [ ] 让 `/glibc/basic_testcode.sh` 可以运行

## P4 — 性能与压力测试

- [ ] 记录并跟踪 EXT4 phase 1 的 `huge_write` 性能回退（当前约 256KiB/s，对比旧 `easy-fs` 约 549KiB/s）
- [ ] 分析 `huge_write` 在 EXT4 路径上的瓶颈（分配、flush、缓存、写入粒度）
- [ ] 优化 EXT4 顺序写路径，让 `huge_write` 不再明显慢于旧 `easy-fs`
- [ ] 推进 `iozone`
- [ ] 推进 `unixbench`
- [ ] 推进 `lmbench`
- [ ] 推进 `iperf`
- [ ] 推进 `netperf`
- [ ] 推进 `cyclictest`
- [ ] 推进 `ltp`

## P5 — LoongArch

- [ ] 阶段 0：冻结 LoongArch 采用路线
  - [ ] 记录结论：采用 `RocketOS` 风格的内置 `arch/` 拆分作为主线，吸收 `NighthawkOS` 的小 HAL facade 组织方式
  - [ ] 记录结论：`reference-project/polyhal` 只作为设计/代码参考，不先接入完整 `polyhal-boot` / `polyhal-trap` / `polyhal` runtime
  - [ ] 复查可借用点：LoongArch `_start`、DMW/MMU 初始化、TLB refill、CSR timer、GED shutdown、virtio-pci 块设备、syscall register ABI
- [ ] 阶段 1：先做 RISC-V 行为不变的架构拆分
  - [ ] 新增 `os/src/arch/mod.rs`，用 `#[cfg(target_arch = ...)]` 选择 `riscv64` / `loongarch64`
  - [ ] 新增 `os/src/arch/riscv64/`，先迁入当前 `entry.asm`、`trap.rs` + `trap/`、`timer.rs`、`sbi.rs`、`boards/qemu.rs`
  - [ ] 让 generic kernel 只通过 `crate::arch` 调用低层入口：board init、irq dispatch、timer、shutdown、trap init、page-table/TLB helper
  - [ ] 保持当前 RISC-V 启动契约不变：`rust_main(hart_id, dtb_addr)`、DTB 解析、可选设备日志、`x0/x1` 磁盘顺序、EXT4 根挂载
  - [ ] 第一轮不引入 proc macro；只有重复 `cfg` 变多后再考虑本地化 `define_arch_mods!()` 风格宏
  - [ ] 验证 `make fmt`、`make all`、`CARGO_NET_OFFLINE=true make all`、`make run-rv-dev`、`make run-rv`
- [ ] 阶段 2：打通 LoongArch 构建与提交产物
  - [ ] 根 `Makefile` 支持 `ARCH=loongarch64`，新增 `kernel-la` 目标
  - [ ] 根 `make all` 同时产出 `kernel-rv` 和 `kernel-la`，但不破坏当前 `kernel-rv` / `disk.img` 缓存规则
  - [ ] `os/Makefile` 支持 `loongarch64-unknown-none`、`qemu-system-loongarch64`、`virtio-blk-pci`、`virtio-net-pci`
  - [ ] 新增 `os/src/linker-loongarch64.ld` 或等价 linker 生成规则；不要复用 RISC-V `linker-qemu.ld`
  - [ ] `user/Makefile` 支持 `loongarch64-unknown-none`
  - [ ] `user` 侧 syscall wrapper 增加 LoongArch 汇编路径和寄存器约定
  - [ ] 将 LoongArch 所需 crate/toolchain 依赖纳入离线构建路径，避免 `make all` 下载网络依赖
- [ ] 阶段 3：LoongArch 最小内核可启动
  - [ ] 实现 `arch/loongarch64/entry`：设置 boot stack、DMW/早期地址映射、必要 FP/扩展使能，并跳入统一 Rust 入口
  - [ ] 实现 `arch/loongarch64/console`：先保证 QEMU `-nographic` 下能输出 boot log
  - [ ] 实现 `arch/loongarch64/shutdown`：QEMU virt GED `0x100E001C <- 0x34`
  - [ ] 实现 `arch/loongarch64/time`：读取 counter、设置 one-shot timer、打开 timer interrupt
  - [ ] 实现 `arch/loongarch64/trap`：kernel trap、user trap、syscall trap、timer trap、external irq trap 的最小闭环
  - [ ] 实现 `arch/loongarch64/mm`：页表 PTE flag 转换、页表切换、TLB flush、TLB refill 入口
  - [ ] 实现 `arch/loongarch64/context`：TrapContext / task context switch / trap return，保证 `fork/execve/wait4` 基础路径可接上
  - [ ] 验证 QEMU LoongArch 能启动到内核日志，并能主动 shutdown
- [ ] 阶段 4：LoongArch 设备与文件系统路径
  - [ ] 明确 QEMU LoongArch virt 设备模型：块设备优先走 PCI virtio，而不是 RISC-V 的 MMIO virtio
  - [ ] 接入 LoongArch PCI/virtio block 发现，至少识别 `x0 = sdcard-la.img`
  - [ ] 保持文件系统上层接口不分叉：LoongArch 复用当前 EXT4 / mount / path / fd 逻辑
  - [ ] 验证从 `sdcard-la.img` 挂载根目录并读取 `/musl`、`/glibc`、测试脚本
  - [ ] 验证 optional `x1` 辅助盘在 LoongArch 下的动态挂载路径
- [ ] 阶段 5：LoongArch 用户态与 submit runner
  - [ ] 产出 LoongArch 用户程序 ELF，并确认 ELF loader 识别 `EM_LOONGARCH`
  - [ ] 对齐 LoongArch 用户态入口、栈、TLS、syscall 返回值、errno 负值约定
  - [ ] 补齐 LoongArch musl BusyBox 启动所需的动态链接器路径或兼容路径
  - [ ] 新增或泛化 submit runner：同一套 runner 能按 `basic-musl` / `busybox-musl` / `glibc` 等组名输出精确 marker
  - [ ] 验证 `submit-la` 或等价入口按固定顺序串行执行测试组，并在结束后主动 shutdown
- [ ] 阶段 6：LoongArch 验收门槛
  - [ ] `make fmt`
  - [ ] `make kernel-rv`
  - [ ] `make kernel-la`
  - [ ] `make all`
  - [ ] `CARGO_NET_OFFLINE=true make all`
  - [ ] `make run-rv` 不回退
  - [ ] `make run-la` 或等价命令能启动 `sdcard-la.img`
  - [ ] 官方 contest Docker 中验证 `kernel-rv`、`kernel-la`、`disk.img` 产物名正确

## 基础设施与并行研究

- [x] 建立官方 QEMU 启动命令的本地复现脚本（CI `ci-riscv-smoke.yml` 覆盖）
- [x] 建立官方容器里的 smoke test 脚本
- [x] 建立 `basic` 用例到 syscall 的逐项对照表（`develop-guide/linux-syscall-implementation-survey.md`）
- [x] 对比 `RustOsWhu` / `NighthawkOS` 的提交路径并提炼可复用做法（`develop-guide/reference-project-notes.md`）
- [x] 评估 EXT4 方案的许可证、维护成本和提交打包方式（`develop-guide/lwext4-rust-research.md` + `ext4-phase1-migration-and-validation.md`）
- [x] 更合适比赛开发的 github ci
- [x] 升级 dependencies 😄（commit `c001a72`）
