#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const skillDir = path.resolve(scriptDir, "..");
const defaultInventoryJson = path.join(skillDir, "references", "local-fork-features.json");
const defaultMarkdown = path.join(skillDir, "references", "local-fork-features.md");
const defaultStateFile = path.join(skillDir, "STATE.md");
const commandOptions = {
  discover: new Set(["repo", "inventory", "state-file", "base-ref", "merge-base-ref", "head-ref", "output"]),
  "merge-candidates": new Set(["dir", "output"]),
  promote: new Set(["inventory", "candidate", "output"]),
  render: new Set(["repo", "inventory", "markdown"]),
  check: new Set(["repo", "inventory", "output"]),
  refresh: new Set(["repo", "inventory", "markdown"]),
};

function parseArgs(argv) {
  const [command, ...rest] = argv;
  const supported = ["discover", "merge-candidates", "promote", "render", "check", "refresh"];
  if (!supported.includes(command)) {
    throw new Error(
      "usage: local_fork_feature_audit.mjs <discover|merge-candidates|promote|render|check|refresh> [--flag value]",
    );
  }

  const options = {};
  const allowedOptions = commandOptions[command];
  for (let index = 0; index < rest.length; index += 1) {
    const flag = rest[index];
    if (!flag.startsWith("--")) {
      throw new Error(`unexpected argument: ${flag}`);
    }
    const optionName = flag.slice(2);
    if (!allowedOptions.has(optionName)) {
      throw new Error(`unknown flag for ${command}: ${flag}`);
    }
    const value = rest[index + 1];
    if (value == null || value.startsWith("--")) {
      throw new Error(`missing value for ${flag}`);
    }
    if (optionName in options) {
      throw new Error(`duplicate flag for ${command}: ${flag}`);
    }
    options[optionName] = value;
    index += 1;
  }

  if (command === "discover" && options["base-ref"] && options["merge-base-ref"]) {
    throw new Error("discover accepts either --base-ref or --merge-base-ref, not both");
  }

  return { command, options };
}

function readText(filePath) {
  return fs.readFileSync(filePath, "utf8");
}

function writeTextIfChanged(filePath, text) {
  const current = fs.existsSync(filePath) ? readText(filePath) : null;
  if (current === text) {
    return false;
  }
  fs.writeFileSync(filePath, text, "utf8");
  return true;
}

function readJson(filePath) {
  return JSON.parse(readText(filePath));
}

function writeJsonIfChanged(filePath, value) {
  const serialized = `${JSON.stringify(value, null, 2)}\n`;
  return writeTextIfChanged(filePath, serialized);
}

function ensureString(value, fieldName) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`expected non-empty string for ${fieldName}`);
  }
}

function validateCheck(check, owner) {
  if (typeof check !== "object" || check === null) {
    throw new Error(`invalid check in ${owner}`);
  }
  ensureString(check.type, `${owner}.checks[].type`);
  ensureString(check.path, `${owner}.checks[].path`);
  if (check.type === "regex") {
    ensureString(check.pattern, `${owner}.checks[].pattern`);
    return;
  }
  if (check.type === "exists") {
    return;
  }
  throw new Error(`unsupported check type in ${owner}: ${check.type}`);
}

function validateFeature(feature) {
  if (typeof feature !== "object" || feature === null) {
    throw new Error("feature must be an object");
  }
  ensureString(feature.id, "feature.id");
  ensureString(feature.kind, `feature(${feature.id}).kind`);
  ensureString(feature.area, `feature(${feature.id}).area`);
  ensureString(feature.summary, `feature(${feature.id}).summary`);
  ensureString(feature.better_when, `feature(${feature.id}).better_when`);
  if (!Array.isArray(feature.checks) || feature.checks.length === 0) {
    throw new Error(`feature(${feature.id}).checks must be a non-empty array`);
  }
  feature.checks.forEach((check) => validateCheck(check, `feature(${feature.id})`));
}

function loadInventory(filePath) {
  const inventory = readJson(filePath);
  if (inventory.version !== 1) {
    throw new Error(`unsupported inventory version in ${filePath}: ${inventory.version}`);
  }
  if (!Array.isArray(inventory.features)) {
    throw new Error(`inventory.features must be an array in ${filePath}`);
  }
  const seenIds = new Set();
  inventory.features.forEach(validateFeature);
  inventory.features.forEach((feature) => {
    if (seenIds.has(feature.id)) {
      throw new Error(`duplicate feature id in ${filePath}: ${feature.id}`);
    }
    seenIds.add(feature.id);
  });
  return inventory;
}

function git(repoRoot, args) {
  return execFileSync("git", ["-C", repoRoot, ...args], { encoding: "utf8" }).trimEnd();
}

function clipDetail(text, limit = 120) {
  const collapsed = text.trim().split(/\s+/u).join(" ");
  if (collapsed.length <= limit) {
    return collapsed;
  }
  return `${collapsed.slice(0, limit - 3)}...`;
}

function regexCheck(repoRoot, check) {
  const relPath = check.path;
  const fullPath = path.join(repoRoot, relPath);
  if (!fs.existsSync(fullPath)) {
    return { type: "regex", path: relPath, ok: false, detail: "file is missing" };
  }

  const text = readText(fullPath);
  const match = new RegExp(check.pattern, "ms").exec(text);
  if (!match) {
    return {
      type: "regex",
      path: relPath,
      ok: false,
      detail: `pattern not found: ${check.pattern}`,
    };
  }

  const lineNumber = text.slice(0, match.index).split("\n").length;
  const line = text.split("\n")[lineNumber - 1] ?? "";
  return {
    type: "regex",
    path: relPath,
    ok: true,
    detail: `${relPath}:${lineNumber} ${clipDetail(line)}`,
  };
}

function existsCheck(repoRoot, check) {
  const relPath = check.path;
  const fullPath = path.join(repoRoot, relPath);
  if (!fs.existsSync(fullPath)) {
    return { type: "exists", path: relPath, ok: false, detail: "path is missing" };
  }
  const kind = fs.statSync(fullPath).isDirectory() ? "dir" : "file";
  return {
    type: "exists",
    path: relPath,
    ok: true,
    detail: `${relPath} exists (${kind})`,
  };
}

function runFeatureCheck(repoRoot, feature) {
  const checks = feature.checks.map((check) => {
    if (check.type === "regex") {
      return regexCheck(repoRoot, check);
    }
    return existsCheck(repoRoot, check);
  });

  return {
    featureId: feature.id,
    kind: feature.kind,
    area: feature.area,
    summary: feature.summary,
    betterWhen: feature.better_when,
    ok: checks.every((item) => item.ok),
    checks,
  };
}

function runAudit(repoRoot, inventory) {
  return inventory.features.map((feature) => runFeatureCheck(repoRoot, feature));
}

function buildAuditMarkdown(results) {
  const passed = results.filter((item) => item.ok).length;
  const lines = [
    `- overall: \`${passed}/${results.length}\` passed`,
    "",
    "| ID | Status | Area |",
    "| --- | --- | --- |",
  ];

  for (const result of results) {
    lines.push(`| \`${result.featureId}\` | \`${result.ok ? "PASS" : "FAIL"}\` | ${result.area} |`);
  }

  for (const result of results) {
    lines.push(
      "",
      `### \`${result.featureId}\``,
      `- status: \`${result.ok ? "PASS" : "FAIL"}\``,
      `- kind: \`${result.kind}\``,
      `- summary: ${result.summary}`,
      `- better_when: ${result.betterWhen}`,
      "- evidence:",
    );
    for (const check of result.checks) {
      lines.push(`  - \`${check.ok ? "ok" : "missing"}\` \`${check.path}\`: ${check.detail}`);
    }
  }

  return lines.join("\n");
}

function buildMarkdownDocument(inventory, auditMarkdown) {
  const lines = [
    "# Local Fork Features",
    "",
    "这个文件由 `local-fork-features.json` 渲染生成，不手工编辑。",
    "",
    "## 文件角色",
    "",
    "- 权威基线：`/workspace/.codex/skills/sync-openai-codex-pr/references/local-fork-features.json`",
    "- 展示报告：当前文件",
    "- 候选变更：默认放在临时路径，由 `discover` 产出、经人工或主代理审阅后再 `promote`",
    "",
    "## 命令",
    "",
    "```bash",
    "node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs discover --repo /workspace --base-ref <sha> --head-ref HEAD --output /tmp/sync-openai-codex-pr-discover.json",
    "node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs merge-candidates --dir /tmp/sync-openai-codex-pr-candidates --output /tmp/sync-openai-codex-pr-candidate-ops.json",
    "node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs promote --candidate /tmp/sync-openai-codex-pr-candidate-ops.json",
    "node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs render --repo /workspace",
    "node /workspace/.codex/skills/sync-openai-codex-pr/scripts/local_fork_feature_audit.mjs check --repo /workspace",
    "```",
    "",
    "`refresh` 是 `render --repo <repo>` 的兼容别名。",
    "`discover` 默认只会从 `STATE.md:last_sync_commit` 推断范围，而且该提交必须仍是 `HEAD` 的祖先。",
    "不会再隐式回退到 `last_synced_sha`；如果你刻意要看更宽的区间，显式传 `--base-ref <last_synced_sha>` 或 `--merge-base-ref <ref>`。",
    "`--base-ref` 和 `--merge-base-ref` 互斥；脚本会拒绝含糊调用。",
    "`merge-candidates` 会把子代理目录里的 candidate ops 合并成一个待审阅文件；同一 feature id 出现互相矛盾的 upsert/remove 会直接失败。",
    "",
    "## Candidate Ops Shape",
    "",
    "```json",
    "{",
    '  "operations": [',
    '    { "action": "upsert", "feature": { "...": "full feature object" } },',
    '    { "action": "remove", "id": "obsolete-feature-id", "reason": "why it is obsolete" }',
    "  ]",
    "}",
    "```",
    "",
    "## Approved Baseline",
    "",
    "| ID | Kind | Area |",
    "| --- | --- | --- |",
  ];

  for (const feature of inventory.features) {
    lines.push(`| \`${feature.id}\` | \`${feature.kind}\` | ${feature.area} |`);
  }

  for (const feature of inventory.features) {
    lines.push(
      "",
      `### \`${feature.id}\``,
      `- summary: ${feature.summary}`,
      `- better_when: ${feature.better_when}`,
      "- checks:",
    );
    for (const check of feature.checks) {
      if (check.type === "regex") {
        lines.push(`  - \`regex\` \`${check.path}\`: \`${check.pattern}\``);
      } else {
        lines.push(`  - \`exists\` \`${check.path}\``);
      }
    }
  }

  lines.push("", "## Latest Audit", "", auditMarkdown);
  return `${lines.join("\n")}\n`;
}

function printFailures(results) {
  const failed = results.filter((item) => !item.ok);
  if (failed.length === 0) {
    return;
  }

  console.error("Missing or overwritten local fork features detected:");
  for (const result of failed) {
    console.error(`- ${result.featureId} (${result.area})`);
    for (const check of result.checks) {
      if (!check.ok) {
        console.error(`  * ${check.path}: ${check.detail}`);
      }
    }
  }
}

function writeOutput(text, outputPath) {
  if (outputPath) {
    writeTextIfChanged(path.resolve(outputPath), text.endsWith("\n") ? text : `${text}\n`);
    return;
  }
  console.log(text);
}

function readStateScalar(stateText, fieldName) {
  const match = stateText.match(new RegExp(`^- ${fieldName}: (.+)$`, "m"));
  if (!match) {
    return null;
  }
  const value = match[1].trim();
  if (!value || value === "<none>") {
    return null;
  }
  return value;
}

function readStateMetadata(stateFile) {
  if (!fs.existsSync(stateFile)) {
    return {};
  }
  const stateText = readText(stateFile);
  return {
    last_sync_commit: readStateScalar(stateText, "last_sync_commit"),
    last_synced_base_branch: readStateScalar(stateText, "last_synced_base_branch"),
    last_synced_sha: readStateScalar(stateText, "last_synced_sha"),
  };
}

function isAncestor(repoRoot, ancestorRef, descendantRef) {
  try {
    execFileSync("git", ["-C", repoRoot, "merge-base", "--is-ancestor", ancestorRef, descendantRef], {
      encoding: "utf8",
      stdio: "ignore",
    });
    return true;
  } catch (error) {
    if (typeof error === "object" && error !== null && "status" in error && error.status === 1) {
      return false;
    }
    throw error;
  }
}

function resolveDiscoverBaseRef(repoRoot, options, headRef) {
  if (options["base-ref"]) {
    return {
      input: options["base-ref"],
      resolved: git(repoRoot, ["rev-parse", options["base-ref"]]),
      strategy: "explicit-base-ref",
    };
  }

  if (options["merge-base-ref"]) {
    return {
      input: `merge-base(${options["merge-base-ref"]},${headRef})`,
      resolved: git(repoRoot, ["merge-base", options["merge-base-ref"], headRef]),
      strategy: "explicit-merge-base",
    };
  }

  const stateFile = options["state-file"] ? path.resolve(options["state-file"]) : defaultStateFile;
  const state = readStateMetadata(stateFile);
  if (state.last_sync_commit) {
    const resolvedSyncCommit = git(repoRoot, ["rev-parse", state.last_sync_commit]);
    if (!isAncestor(repoRoot, resolvedSyncCommit, headRef)) {
      const branchDetail = state.last_synced_base_branch
        ? ` (recorded base branch: ${state.last_synced_base_branch})`
        : "";
      throw new Error(
        `state ${stateFile} last_sync_commit ${state.last_sync_commit}${branchDetail} is not an ancestor of ${headRef}; discover cannot safely infer a local-only range. Pass --base-ref <trusted-local-commit> or --merge-base-ref <ref> for deliberate broad mode.`,
      );
    }
    return {
      input: `state:last_sync_commit:${stateFile}`,
      resolved: resolvedSyncCommit,
      strategy: "state:last_sync_commit",
    };
  }

  if (state.last_synced_sha) {
    throw new Error(
      `state ${stateFile} only has last_synced_sha ${state.last_synced_sha}; discover no longer defaults to upstream baselines. Pass --base-ref ${state.last_synced_sha} only if you intentionally want that broader range, or record a valid last_sync_commit first.`,
    );
  }

  throw new Error(
    "discover requires --base-ref, --merge-base-ref, or a state file with last_sync_commit that is an ancestor of HEAD",
  );
}

function parseCommitFiles(repoRoot, sha) {
  const output = git(repoRoot, ["show", "--format=", "--name-only", "--diff-filter=ACDMRT", sha]);
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function normalizeBuckets(files) {
  const groups = new Map();
  for (const file of files) {
    const parts = file.split("/");
    const group = parts[0] === "codex-rs" && parts.length >= 2 ? `${parts[0]}/${parts[1]}` : parts[0];
    const bucket = groups.get(group) ?? [];
    bucket.push(file);
    groups.set(group, bucket);
  }
  return [...groups.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([group, groupFiles]) => ({ group, files: groupFiles.sort() }));
}

function pathMatchesFeature(filePath, feature) {
  return feature.checks.some((check) => {
    if (check.type === "exists") {
      return filePath === check.path || filePath.startsWith(`${check.path}/`);
    }
    return filePath === check.path;
  });
}

function discover(repoRoot, inventory, options) {
  const headRef = options["head-ref"] || "HEAD";
  const base = resolveDiscoverBaseRef(repoRoot, options, headRef);
  const resolvedHead = git(repoRoot, ["rev-parse", headRef]);
  const changedFilesOutput = git(repoRoot, ["diff", "--name-only", `${base.resolved}..${resolvedHead}`]);
  const changedFiles = changedFilesOutput
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .sort();
  const commitRecords = git(
    repoRoot,
    ["log", "--reverse", "--format=%H%x1f%s%x1f%b%x1e", `${base.resolved}..${resolvedHead}`],
  )
    .split("\u001e")
    .map((record) => record.trim())
    .filter(Boolean);

  const commits = commitRecords.map((record) => {
    const [sha, title, body = ""] = record.split("\u001f");
    const files = parseCommitFiles(repoRoot, sha).sort();
    return { sha, title, body: body.trim(), files };
  });

  const touchedFeatures = inventory.features
    .map((feature) => {
      const files = changedFiles.filter((filePath) => pathMatchesFeature(filePath, feature));
      if (files.length === 0) {
        return null;
      }
      const commitShas = commits
        .filter((commit) => commit.files.some((filePath) => files.includes(filePath)))
        .map((commit) => commit.sha);
      return {
        feature_id: feature.id,
        summary: feature.summary,
        files,
        commit_shas: commitShas,
      };
    })
    .filter(Boolean);

  const uncoveredPaths = changedFiles.filter(
    (filePath) => !inventory.features.some((feature) => pathMatchesFeature(filePath, feature)),
  );

  return {
    version: 1,
    repo_root: repoRoot,
    range: {
      base_ref_input: base.input,
      base_strategy: base.strategy,
      head_ref_input: headRef,
      resolved_base: base.resolved,
      resolved_head: resolvedHead,
    },
    stats: {
      commit_count: commits.length,
      changed_file_count: changedFiles.length,
      touched_feature_count: touchedFeatures.length,
      uncovered_path_count: uncoveredPaths.length,
    },
    commits,
    touched_features: touchedFeatures,
    uncovered_paths: uncoveredPaths,
    uncovered_path_groups: normalizeBuckets(uncoveredPaths),
  };
}

function validateOperation(operation) {
  if (typeof operation !== "object" || operation === null) {
    throw new Error("candidate operation must be an object");
  }
  ensureString(operation.action, "candidate.operation.action");
  if (operation.action === "upsert") {
    validateFeature(operation.feature);
    return;
  }
  if (operation.action === "remove") {
    ensureString(operation.id, "candidate.operation.id");
    if (operation.reason != null) {
      ensureString(operation.reason, "candidate.operation.reason");
    }
    return;
  }
  throw new Error(`unsupported candidate operation: ${operation.action}`);
}

function readCandidateOperations(candidatePath) {
  const candidate = readJson(candidatePath);
  if (!Array.isArray(candidate.operations)) {
    throw new Error(`candidate file must contain an operations array: ${candidatePath}`);
  }
  candidate.operations.forEach(validateOperation);
  return candidate.operations;
}

function mergeCandidateOperations(candidateDir) {
  if (!fs.existsSync(candidateDir)) {
    throw new Error(`candidate directory is missing: ${candidateDir}`);
  }
  if (!fs.statSync(candidateDir).isDirectory()) {
    throw new Error(`candidate path is not a directory: ${candidateDir}`);
  }

  const files = fs
    .readdirSync(candidateDir)
    .filter((fileName) => fileName.endsWith(".json"))
    .sort()
    .map((fileName) => path.join(candidateDir, fileName));

  if (files.length === 0) {
    throw new Error(`candidate directory has no .json files: ${candidateDir}`);
  }

  const mergedOperations = [];
  const byFeatureId = new Map();

  for (const filePath of files) {
    const operations = readCandidateOperations(filePath);
    for (const operation of operations) {
      const featureId = operation.action === "upsert" ? operation.feature.id : operation.id;
      const serialized = JSON.stringify(operation);
      const existing = byFeatureId.get(featureId);
      if (!existing) {
        byFeatureId.set(featureId, { serialized, action: operation.action, source: filePath });
        mergedOperations.push(operation);
        continue;
      }
      if (existing.serialized === serialized) {
        continue;
      }
      throw new Error(
        `conflicting candidate operations for feature ${featureId}: ${existing.source} (${existing.action}) vs ${filePath} (${operation.action})`,
      );
    }
  }

  return {
    version: 1,
    source_dir: candidateDir,
    source_files: files,
    operations: mergedOperations,
  };
}

function promote(inventory, candidatePath) {
  const operations = readCandidateOperations(candidatePath);

  let features = [...inventory.features];
  for (const operation of operations) {
    if (operation.action === "remove") {
      features = features.filter((feature) => feature.id !== operation.id);
      continue;
    }

    const nextFeature = operation.feature;
    const index = features.findIndex((feature) => feature.id === nextFeature.id);
    if (index === -1) {
      features.push(nextFeature);
    } else {
      features[index] = nextFeature;
    }
  }

  return { version: 1, features };
}

function main() {
  const { command, options } = parseArgs(process.argv.slice(2));
  const inventoryFile = path.resolve(options.inventory || defaultInventoryJson);
  const inventory = loadInventory(inventoryFile);

  if (command === "discover") {
    if (!options.repo) {
      throw new Error("discover requires --repo <repo-root>");
    }
    const repoRoot = path.resolve(options.repo);
    const result = discover(repoRoot, inventory, options);
    const text = `${JSON.stringify(result, null, 2)}\n`;
    if (options.output) {
      writeTextIfChanged(path.resolve(options.output), text);
      console.log(`Wrote ${path.resolve(options.output)}`);
    } else {
      process.stdout.write(text);
    }
    return;
  }

  if (command === "merge-candidates") {
    if (!options.dir) {
      throw new Error("merge-candidates requires --dir <candidate-dir>");
    }
    const merged = mergeCandidateOperations(path.resolve(options.dir));
    const text = `${JSON.stringify(merged, null, 2)}\n`;
    if (options.output) {
      writeTextIfChanged(path.resolve(options.output), text);
      console.log(`Wrote ${path.resolve(options.output)}`);
    } else {
      process.stdout.write(text);
    }
    return;
  }

  if (command === "promote") {
    if (!options.candidate) {
      throw new Error("promote requires --candidate <candidate-ops.json>");
    }
    const promoted = promote(inventory, path.resolve(options.candidate));
    const outputFile = path.resolve(options.output || inventoryFile);
    const changed = writeJsonIfChanged(outputFile, promoted);
    console.log(changed ? `Updated ${outputFile}` : `No changes for ${outputFile}`);
    return;
  }

  if (command === "check") {
    if (!options.repo) {
      throw new Error("check requires --repo <repo-root>");
    }
    const results = runAudit(path.resolve(options.repo), inventory);
    const report = buildAuditMarkdown(results);
    writeOutput(report, options.output ? path.resolve(options.output) : null);
    printFailures(results);
    process.exit(results.every((item) => item.ok) ? 0 : 1);
  }

  const renderLike = command === "render" || command === "refresh";
  if (renderLike) {
    if (!options.repo) {
      throw new Error(`${command} requires --repo <repo-root>`);
    }
    const repoRoot = path.resolve(options.repo);
    const results = runAudit(repoRoot, inventory);
    const report = buildAuditMarkdown(results);
    const document = buildMarkdownDocument(inventory, report);
    const markdownPath = path.resolve(options.markdown || defaultMarkdown);
    const changed = writeTextIfChanged(markdownPath, document);
    console.log(changed ? `Rendered ${markdownPath}` : `No changes for ${markdownPath}`);
    printFailures(results);
    process.exit(results.every((item) => item.ok) ? 0 : 1);
  }
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
