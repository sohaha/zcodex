# sync-zoffsec-reference state

- upstream_repo: https://github.com/ryfineZ/codex-session-patcher
- upstream_ref: af401d3e53f3836788c4326e01499d7d7946ceb1
- last_synced_hash: af401d3e53f3836788c4326e01499d7d7946ceb1
- last_synced_at_utc: 2026-04-19T08:41:43Z
- notes: selective sync against upstream `main` landed for Codex-specific cleanup behavior only. Audited sources were `README.md`, `core/formats.py`, `core/detector.py`, `core/patcher.py`, and `ctf_config/templates.py`. Landed parity updates: `clean_zoffsec_rollout()` now rewrites both `event_msg.agent_message` and `event_msg.task_complete.last_agent_message`, and refusal detection now follows the upstream two-level model (strong full-text phrases + weak head-only keywords). Intentional local divergence remains: native `codex zoffsec` subcommand UX, local `default/web/reverse` templates, fixed replacement text instead of AI rewrite, and no installer/Web UI/Claude/OpenCode scope.
