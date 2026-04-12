# sync-openai-codex-pr state

- upstream_repo: https://github.com/openai/codex.git
- upstream_branch: main
- last_synced_sha: 824ec94eab098375dcbb9cf2da1d666fb68ad40f
- last_synced_at_utc: 2026-04-11T03:26:57Z
- last_synced_base_branch: web
- last_sync_commit: cbffb8c32
- notes: 最近一次“可准确审计 upstream SHA”的落地同步仍是 cbffb8c32（7bbe3b6..824ec94）。其后 web 已落地 e08014806 / ea467a787 这一轮更晚同步，并已按用户决定跟随 upstream 删除 account；经文件级核对，当前代码至少吸收了 0bdeab330 及之前的多项 upstream 变更，但由于 ea467a787 不是 merge-parent 型同步提交，且提交正文未记录 target SHA，当前仍需重新核定这轮同步的精确 upstream 上界，故暂不更新 last_synced_sha 以避免写入虚假基线。
