# OpenVSX Upstream Sync Checklist

每次执行 `sync-openvsx-upstream` 都先过这份清单。

## Baseline

- 读取 `/workspace/.codex/skills/sync-openvsx-upstream/STATE.md`
- 确认上游仓库是 `https://github.com/eclipse-openvsx/openvsx.git`
- 确认上游分支是 `main`
- 确认本次目标目录是 `/workspace/third_party/openvsx`

## Audit

- 审计 `previous_sha..openvsx_sha` 的纯上游变化
- 审计当前 vendored 目录相对新上游快照的本地偏移
- 列出会被覆盖、删除或需要重放本地补丁的文件

## Implementation

- 在独立 worktree 中进行
- 使用缓存上游仓库导出快照，不要把 vendored 目录变成 Git 仓库
- 用 `rsync -a --delete --exclude '.git'` 覆盖到 `third_party/openvsx`
- 必要时重放本地补丁
- 落地后更新 `STATE.md`

## Validation

- `test ! -e third_party/openvsx/.git`
- `git status --short -- third_party/openvsx`
- spot check 关键文件
- 如未运行更多验证，在总结里明确写出

## Summary

- previous recorded baseline
- new upstream sha
- upstream delta summary
- preserved local patches
- notable conflicts
- validation completed
- remaining risk
