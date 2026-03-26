# Tool Routing (Native TLDR + RTK)

- Prefer the built-in `tldr` tool for structured code understanding before broad manual file reads.
- Use `tldr` `tree`/`context`/`impact`/`semantic` to map symbols, relationships, blast radius, and behavior-oriented code search.
- Use built-in read-only `rtk_*` tools (`rtk_read`, `rtk_grep`, `rtk_find`, `rtk_diff`, `rtk_json`, `rtk_deps`, `rtk_log`, `rtk_ls`, `rtk_tree`, `rtk_wc`, `rtk_git_status`, `rtk_git_diff`, `rtk_git_show`, `rtk_git_log`, `rtk_git_branch`, `rtk_git_stash`, `rtk_git_worktree`, `rtk_summary`, `rtk_err`) for token-efficient inspection, search, listing, counting, git introspection, and concise diff/error summaries.
- Route mutating git work and external/infra/network RTK commands through `shell_command` + `codex rtk ...` instead of a dedicated built-in tool.
