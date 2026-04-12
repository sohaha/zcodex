# 2026-04-12 Windows 安装器 bundle parity 反思

## 背景
- 用户追问为什么 macOS 看起来只装 `codex-cli`，Windows 却像是需要单独 `native-tldr`。
- 代码事实并非“Windows 必须独立发 native-tldr”，而是 `install.ps1` 仍停留在手写下载若干 `.exe` 的旧分发模式，和 `install.sh` 已采用的 npm bundle/vendor 布局不一致。
- 这会放大平台认知偏差：用户更容易把 Windows 的额外 helper 误读成“多装了一个 native-tldr 组件”。

## 本轮有效做法
- 先确认产品事实：`codex-native-tldr` 已编入 `codex.exe`，Windows 独立分发的是 sandbox helper，不是 native-tldr。
- 将 `scripts/install/install.ps1` 收口到和 `install.sh` 相同的优先级：先拿 `codex-npm-win32-*.tgz`，按 `package/vendor/<target>/` 安装 `codex.exe`、Windows helper 和 `rg.exe`。
- 保留 Windows 历史兼容路径：bundle 缺失时再回退到 `codex-<target>.exe.zip`，最后才退到逐个 `.exe` 下载。
- 让 `CODEX_INSTALL_DIR` 与 `CODEX_BASE_URL` 在 PowerShell 安装器里生效，保持和 Unix 安装器、CI 本地烟测一致。
- 消除双安装目录/双复制逻辑，只把单一 `InstallDir` 写入 PATH，减少“实际运行的是哪份 codex.exe”这类定位噪音。

## 关键收益
- Windows/macOS/Linux 安装器开始共享同一种发布物语义：优先消费 bundle，而不是平台脚本各自硬编码资产清单。
- 以后 Windows bundle 中新增或移除组件时，`install.ps1` 不必同步维护多份复制逻辑，发布漂移风险更小。
- CI 的本地 http server 烟测终于和真实分发结构一致：PowerShell 安装器会真正消费 `codex-npm-win32-*.tgz` 与 `package/vendor/...`。

## 踩坑
- 看到 Windows 上多了 `codex-command-runner.exe` / `codex-windows-sandbox-setup.exe`，不要直接推断产品被拆成了多个功能 sidecar；先区分 sandbox helper 和 native-tldr。
- PowerShell 安装器如果不支持 `CODEX_BASE_URL` / `CODEX_INSTALL_DIR`，CI 看起来像在做本地烟测，实际却和真实路径、真实下载源脱节。
- 自定义测试资产常用假的 `.exe` 占位文件；安装器若强制执行真实二进制烟测，需要显式处理这种自定义源场景，否则会让安装流程测试卡在无意义的可执行校验上。

## 后续建议
- 后续若发布系统再调整 bundle 结构，应优先把变更收口到 bundle/vender 约定，而不是在平台安装脚本里扩散资产名分支。
- 若要进一步加强 Windows 安装器验证，优先在真实 release artifact 或专用集成测试里覆盖 `codex ztldr languages`，不要依赖伪造 `.exe` 的本地烟测去证明可运行性。
