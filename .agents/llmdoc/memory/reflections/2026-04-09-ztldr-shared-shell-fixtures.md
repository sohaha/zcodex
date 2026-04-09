# 2026-04-09 ztldr shared shell fixtures 反思

## 背景
- `shell_search_rewrite.rs` 的 project corpus summary 之前虽然已经共享了 query 样本，但 shell 命令字符串本身仍靠测试内部的条件分支拼装，变体规则没有进入统一事实源。

## 本轮有效做法
- 在 `core/src/tools/rewrite/test_corpus.rs` 新增：
  - `ShellCorpusCase`
  - `PROJECT_SHELL_CORPUS`
- 让 shell 层直接消费共享的命令 fixture，而不是在测试里根据 pattern 再拼装 `rg` 命令。

## 关键收益
- shell 变体（自然语言引号、成员查询路径、pathlike 参数布局、regex passthrough）现在也是显式测试数据，而不是隐藏在 if/else 里。
- 后续新增 shell 场景时，优先加 fixture，不需要再读测试逻辑猜命令是怎么拼出来的。

## 后续建议
- 如果后面要覆盖更多 `ztok grep` / `find | xargs rg` / `-g` 参数组合，继续沿共享 shell fixture 扩展，不要把命令重写逻辑散回测试函数。
