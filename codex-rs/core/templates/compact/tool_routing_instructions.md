# Tool Routing (Native TLDR + RTK)

- Prefer the built-in `tldr` tool for structured code understanding before broad manual file reads.
- Use `tldr` `tree`/`context`/`impact`/`semantic` to map symbols, relationships, blast radius, and behavior-oriented code search.
- Use built-in `rtk_*` tools for token-efficient inspection, search, listing, counting, git introspection, and concise diff/error summaries. Prefer `rtk_git_status`, `rtk_git_diff`, `rtk_git_show`, `rtk_git_log`, `rtk_git_branch`, `rtk_git_stash`, and `rtk_git_worktree` over `shell_command` + `codex rtk git ...` for read-only git inspection.
- Use `rtk_summary` and `rtk_err` when you need RTK-filtered summaries for another command's output.
- Route mutating git work and external/infra/network RTK commands through `shell_command` + `codex rtk ...` instead of a dedicated built-in tool.
