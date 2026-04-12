# Git Commit 规范

更新日期：2026-04-12

## 1. 现有 hook 约束

```commit-msg
type(scope): summary
```

允许的 `type`：

- `feat`
- `fix`
- `refactor`
- `docs`
- `chore`

## 2. 推荐模板

```commit-msg
<type>(<scope>): <summary>
```

## 3. 推荐 scope

当前仓库已在使用的 scope：

- `board`
- `dtb`
- `virtio-mmio`
- `drivers`
- `build`
- `dep`
- `doc`
- `cleanup`

## 4. 例子

- `fix(board): allow headless qemu boot without gpu devices`
- `fix(block): detect multiple virtio block devices from dtb`
- `feat(submit): add serial runner for testcase shell scripts`
- `fix(initproc): launch submit runner in contest mode`
- `fix(judge): print exact oscomp group markers`
- `feat(poweroff): shutdown after finishing all test groups`
- `fix(syscall): implement getdents64 for basic-musl`
- `fix(syscall): upgrade syscall 56 to openat-compatible semantics`
- `refactor(fs): split testcase disk and local rootfs access`
- `chore(build): add root all target for kernel-rv and kernel-la`
- `chore(build): vendor cargo dependencies for offline builds`
- `docs(plan): simplify contest development plan into task list`
