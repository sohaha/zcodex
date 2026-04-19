# sync-zoffsec-reference state

- upstream_repo: https://github.com/ryfineZ/codex-session-patcher
- upstream_ref: af401d3e53f3836788c4326e01499d7d7946ceb1
- last_synced_hash: af401d3e53f3836788c4326e01499d7d7946ceb1
- last_synced_at_utc: 2026-04-19T08:41:43Z
- notes: audited selective sync against upstream `main` landed for Codex-specific cleanup behavior only. Audited sources were `README.md`, `core/formats.py`, `core/detector.py`, `core/patcher.py`, and `ctf_config/templates.py`. Landed parity updates are limited to the rollout cleaner: `clean_zoffsec_rollout()` rewrites both `event_msg.agent_message` and `event_msg.task_complete.last_agent_message`, and refusal detection follows the upstream two-level model (strong full-text phrases + weak head-only keywords). Important audit boundary: the original local `codex ctf` / `codex zoffsec` command workflow predates this skill and had no contemporaneous synced hash; do not describe the native subcommand, local `default/web/reverse` templates, base-instructions marker injection, or `zoffsec resume` clean hook as full upstream parity. Those remain intentional local divergence, along with fixed replacement text instead of AI rewrite and no installer/Web UI/Claude/OpenCode scope.
