# 反思：本地分叉特性应尽量脱离 upstream 热点文件

## 背景
这次任务不是直接修某个功能，而是补强同步 `openai/codex` 的流程规则。用户明确要求把“新功能和新特性尽量不和上游文件重叠”写进同步技能和仓库协作约束里，目标是降低后续 sync 的反复冲突成本。

## 结论
- 对长期保留的本地特性，不能只看“这次改哪里最方便”；还要把未来持续同步时的重叠面积当成显式成本。
- 如果功能主体可以放进新增本地文件、模块、crate、adapter seam 或命令面，就不要继续堆在 upstream 高频文件里。
- 必须改 upstream 文件时，应只留下最薄的一层桥接，例如命令注册、参数透传、模块导出、trait impl 或配置 wiring。
- 同步技能不应只检查“功能有没有丢”；还应额外审查本地特性的主体是否仍压在 upstream 热点文件里，以及是否存在可迁移到本地新文件的更优落位。

## 这次落地
- `/workspace/.codex/skills/sync-openai-codex-pr/SKILL.md`
  - 新增“本地特性落位与上游重叠控制”规则，要求同步时主动压低长期重叠面积。
- `/workspace/AGENTS.md`
  - 新增仓库级 `Local Fork Feature Placement` 规则，把“主体逻辑优先落在本地新文件”升格为稳定协作约束。

## 后续提醒
- 以后设计本地新能力时，若第一反应是直接改 `codex-rs/tui/src/app.rs`、`codex-rs/cli/src/main.rs`、`codex-rs/core/src/config/mod.rs` 这类 upstream 热点文件，应先停一下，确认是否能改成“热点文件只接线，新文件承载主体”。
- 如果某次同步仍反复在同一热点文件冲突，说明前一轮的本地特性落位已经需要抽离，不应继续把冲突当作一次性的 merge 噪音。
