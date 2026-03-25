# Requirements Confirmation

## Original Request

- Continue in parallel.
- Complete stronger global uniqueness for daemon startup.
- Make startup and daemon form a tighter closed loop around lock / liveness / stale handling.
- Start first-phase stabilization items:
  - shared config
  - daemon health/status
  - session/dirty/reindex coordination

## Clarification Rounds

### Round 1

- Pending questions were narrowed to:
  1. shared config scope
  2. daemon health/status surface
  3. session/dirty/reindex scope

### Round 2

- User instruction: continue based on prior records without waiting for more clarification.
- Assumptions accepted for implementation:
  1. `shared config` first phase uses the minimum set:
     - `daemon.auto_start`
     - `daemon.socket_mode`
     - `semantic.enabled`
     - `semantic.auto_reindex_threshold`
  2. `daemon health/status` first phase includes an internal status model plus CLI/MCP visible status surface.
  3. `session/dirty/reindex` first phase focuses on state coordination and observability, not a full background reindex pipeline yet.

## Scoring

### Round 1 Score: 78/100

- Functional Clarity: 24/30
- Technical Specificity: 20/25
- Implementation Completeness: 18/25
- Business Context: 16/20

### Round 2 Score: 92/100

- Functional Clarity: 28/30
  - Phase-1 deliverables are now constrained to minimum viable stabilization.
- Technical Specificity: 23/25
  - Config, status, and session coordination boundaries are now explicit.
- Implementation Completeness: 22/25
  - Background reindex is intentionally deferred; state visibility is in scope now.
- Business Context: 19/20
  - Priority and delivery boundary are sufficiently clear for implementation.

## Current Confirmed Requirement

- Strengthen native-tldr daemon lifecycle closing around lock, liveness, and stale handling.
- Continue using parallel work where helpful.
- Deliver phase-1 stabilization with:
  - stronger global uniqueness around daemon startup
  - tighter lock/liveness/stale closed loop between launcher and daemon
  - shared config for minimum tldr daemon/semantic knobs
  - daemon health/status surfaced to CLI and MCP
  - session/dirty/reindex coordination and visibility, without full background reindex pipeline

## Current Implementation Snapshot

- Minimum shared config is already wired through CLI, daemon, and MCP via `project/.codex/tldr.toml`.
- Daemon health/status is already exposed on all three surfaces and now includes explicit diagnostic hints:
  - `healthy`
  - `stale_socket`
  - `stale_pid`
  - `health_reason`
  - `recovery_hint`
- Launcher/daemon closed loop is tightened so stale cleanup only happens when the project lock is not held, avoiding interference with another process that is already starting the daemon.
- Session/dirty/reindex remains phase-1 scoped to visibility and coordination state (`dirty_file_threshold`, `reindex_pending`), not background indexing automation yet.

## Assumptions

- Unix remains the primary path for daemon lifecycle improvements in this phase.
- The user wants minimum viable but production-oriented stabilization, not a speculative redesign.
- Phase-1 prioritizes correctness, observability, and stable interfaces over maximum automation.

## Risks

- Global uniqueness is stronger than before but may still need one more iteration if OS-level locking and stale recovery interact badly under crash scenarios.
- Shared config may require follow-up wiring into more surfaces once background reindexing is introduced later.
- Status exposure is now sufficient for phase-1 diagnostics, but some daemon/session invariants are still placeholders and may need later hardening.
