# Fork vs upstream Codex

This repository is a fork of `openai/codex` that keeps the core CLI/TUI experience but focuses on a few extra areas:

- multi-model support (via pluggable providers),
- in-process agent teams and long-running orchestration,
- a local Web UI served by `codex serve`,
- stronger lifecycle hooks and scheduled tasks.

This document summarizes the main differences you are likely to notice when coming from the upstream project.

## Scope and positioning

- **Upstream (`openai/codex`)**: focuses on a first-party Codex CLI that talks to OpenAI models, with CLI/TUI as the primary interaction surfaces.
- **This fork (`stellarlinkco/codex`)**: keeps CLI/TUI but treats Codex as a “Rust OpenCode” base that can orchestrate multiple providers and agents, including Anthropic models, and expose everything through a browser-based UI (`codex serve`).

The intent is that most upstream workflows continue to “just work” here, while additional features are available when you opt into them.

## Model providers and Anthropic support

What this fork adds:

- **Pluggable model providers** in `config.toml` via the `[model_providers.*]` section (see `docs/config.md` for details).
- A **documented Anthropic provider example**:
  - configure `model_providers.anthropic` with `base_url`, `env_key = "ANTHROPIC_API_KEY"`, and `wire_api = "anthropic"`;
  - select it via `model_provider = "anthropic"` and a Claude model name (for example `claude-sonnet-4-5`).
- Provider overrides can be applied **per agent role** using `~/.codex/agents/*.toml`.

In practice this means:

- You can mix OpenAI and non-OpenAI models in the same Codex installation.
- Existing CLI/TUI flows still work; you only opt into other providers when you configure them.

## `codex serve` Web UI

This fork adds a new CLI subcommand:

```bash
codex serve [OPTIONS]
```

Key behavior (see `docs/prd-codex-serve.md` for the full design):

- Starts a local HTTP server (Axum-based) with:
  - a browser **Web UI** for chat, session management, tool approvals, and terminal integration;
  - an **SSE event stream** for live updates;
  - a **WebSocket** endpoint used for the integrated terminal/PTY.
- Serves the Web UI as **embedded static assets** (single-binary distribution).
- Uses a **random per-process auth token** by default:
  - HTTP requests must send `Authorization: Bearer <token>` or `?token=<token>`;
  - SSE and WebSocket connections validate the token on connect.
- Binds to `127.0.0.1` by default, with an explicit warning if you bind to `0.0.0.0`.

From a user’s perspective:

- Upstream focuses on CLI/TUI; this fork also lets you open a browser and use a full Web UI backed by the same core runtime.

## Agent Teams and multi-agent orchestration

This fork formalizes in-process multi-agent workflows as **Agent Teams** (see `docs/agent-teams.md` and `docs/plans/2026-03-06-codex-swarm-architecture.md`):

- New tools:
  - `spawn_team`, `wait_team`, `close_team`, `team_cleanup`;
  - `team_task_list`, `team_task_claim`, `team_task_claim_next`, `team_task_complete`;
  - `team_message`, `team_broadcast`, `team_ask_lead`, `team_inbox_pop`, `team_inbox_ack`.
- A **team lifecycle model**:
  - create a team with multiple named members (roles) and tasks;
  - persist team config and task state under `$CODEX_HOME/teams/<team_id>` and `$CODEX_HOME/tasks/<team_id>`;
  - coordinate work via durable inboxes and task claims.
- Support for **background** and **worktree-isolated** members for safer workspace changes.

Compared with a single-agent workflow:

- You can keep using `spawn_agent` for simple cases.
- When tasks are naturally parallel or role-based, you can promote them to an Agent Team and let Codex manage concurrency, task distribution, and durable state.

## Hooks and lifecycle integration

This fork expands and documents a **hook system** that lets you attach handlers at many lifecycle points (see `docs/hooks.md` and the Hooks section in `docs/config.md`):

- Supported events include:
  - `session_start`, `session_end`, `user_prompt_submit`, `stop`;
  - `pre_tool_use`, `permission_request`, `post_tool_use`, `post_tool_use_failure`;
  - `notification`, `config_change`, `pre_compact`;
  - multi-agent events such as `subagent_start`, `subagent_stop`, `teammate_idle`, `task_completed`;
  - workspace events `worktree_create`, `worktree_remove`.
- Three handler types:
  - **command**: run a process, read JSON from `stdin`, optionally emit JSON on `stdout` (can block with exit code `2`);
  - **prompt**: send a one-off evaluation prompt to a model, expecting `{ "ok": bool, "reason"?: string }`;
  - **agent**: spawn a verifier subagent that uses tools and returns the same `{ok, reason}` shape.
- Hooks support:
  - **matchers** by `matcher` regex, `tool_name`, or `tool_name_regex`;
  - **async command hooks** for background, non-blocking processing.

Typical uses:

- Enforce policy on risky tools (for example `shell`, `exec`) before they execute.
- Block or annotate certain tool calls or subagent spawns.
- Inject additional system messages or context for the next turn.

## Scheduled tasks and `/loop`

This fork adds:

- **Scheduled-task tools** that can run actions at intervals (controlled by `disable_cron` in `config.toml`).
- A new TUI/CLI slash command `/loop` (see `docs/slash_commands.md`):
  - syntax: `/loop [interval] <prompt>`;
  - `interval` defaults to `10m` and supports `30s`, `10m`, `2h`, `1d`, and natural phrases (“review PR every 2 hours”).
- `/loop` works by rewriting your request into a normal user turn that asks Codex to create a recurring scheduled task.

Effectively, this gives Codex a built-in “cron for conversations” that can:

- periodically check CI or GitHub state,
- re-run diagnostics,
- or nudge you about long-running work items.

## GitHub webhook configuration surface

While `codex github` exists upstream, this fork emphasizes a richer, config-driven setup (documented in `docs/config.md` and `docs/github-outcome-first-overlay.md`):

- A top-level `[github_webhook]` table in `config.toml`:
  - enables/disables the webhook server;
  - configures listen address, allowed repos, minimum permission level;
  - references env vars for secrets (webhook secret, GitHub token, GitHub App credentials);
  - selects event types (issues, pull requests, reviews, comments, push).
- An **experimental “outcome-first overlay”** design for future orchestration (clarify requirements → durable execution → proof → GitHub writeback).

These features are designed so that:

- Native `codex github` behavior stays compatible by default.
- You can opt into more structured, outcome-driven GitHub workflows when desired.

## Compatibility notes

- Core concepts like **threads**, **turns**, tools, and TUI remain aligned with upstream Codex.
- Most upstream configuration options and CLI commands should behave the same here.
- The additional features above are **opt-in**:
  - you can ignore `codex serve`, multi-provider config, Agent Teams, and hooks and still use this fork as “just Codex CLI/TUI”;
  - adopting them is incremental and driven by your workflow needs.

If you are migrating from upstream Codex and run into a behavior difference that is not covered here, please file an issue in this fork so the divergence can be documented or corrected.

