# Clouddev 的 copy-on-write 挂载会遮住镜像内 mise

## 背景
- `web` 分支的 Clouddev 启动后，`/root/.local/bin` 和 `/root/.local/share/mise` 在容器内变成空目录，导致 `mise`、`lnk`、`rustc`、`cargo` 等预装工具全部丢失。
- 同一时间，直接 `docker run docker.cnb.cool/zls-tools/vm/rust:20260410` 可以看到镜像内这些路径和工具都存在，说明问题不在镜像构建，而在运行时挂载。

## 根因
- `/workspace/.cnb.yml` 在 `vscode.clouddev.docker.volumes` 里新增了 `/root/.local/bin:copy-on-write` 和 `/root/.local/share/mise:copy-on-write`。
- CNB 启动后，这两个挂载点会覆盖镜像层里的同名目录，所以镜像里打进去的 `mise`、`lnk` 和 mise installs 会被空卷遮住。
- 同一个文件里的 tag release 配置还把 `/root/.local/share/mise` 挂成了 `copy-on-write`，会让 release 流水线也失去镜像预装的 mise toolchain。

## 证据
- 当前运行中的容器 `mountinfo` 出现独立挂载点：`/root/.local/bin`、`/root/.local/share/mise`。
- `git blame /workspace/.cnb.yml` 显示这两条挂载来自 `93f5052c2 feat(clouddev): mount cargo/rustup volumes and install sccache via cargo`，时间是 2026-04-09。
- 删除这三条挂载后，配置层不再要求平台覆盖这两个目录；修复需要通过重新创建 Clouddev 工作区来生效。

## 经验
- 对基于镜像预装工具链的目录，不要追加 `copy-on-write` 卷，尤其是：
  - `/root/.local/bin`
  - `/root/.local/share/mise`
- 适合挂缓存卷的是可再生数据目录，例如 cargo registry、cargo git、rustup、cache；不适合挂二进制入口目录和预装工具目录。
