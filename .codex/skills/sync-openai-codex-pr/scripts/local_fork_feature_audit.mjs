#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SPEC_START = "<!-- local-fork-feature-spec:start -->";
const SPEC_END = "<!-- local-fork-feature-spec:end -->";
const REPORT_START = "<!-- local-fork-feature-report:start -->";
const REPORT_END = "<!-- local-fork-feature-report:end -->";

function parseArgs(argv) {
  const [command, ...rest] = argv;
  if (!["refresh", "check"].includes(command)) {
    throw new Error("usage: local_fork_feature_audit.mjs <refresh|check> --repo <path> [--inventory <path>]");
  }

  let repo = null;
  let inventory = null;
  for (let index = 0; index < rest.length; index += 1) {
    const flag = rest[index];
    const value = rest[index + 1];
    if (!value) {
      throw new Error(`missing value for ${flag}`);
    }
    if (flag === "--repo") {
      repo = value;
      index += 1;
      continue;
    }
    if (flag === "--inventory") {
      inventory = value;
      index += 1;
      continue;
    }
    throw new Error(`unknown argument: ${flag}`);
  }

  if (!repo) {
    throw new Error("--repo is required");
  }

  return { command, repo, inventory };
}

function inventoryPath(cliInventory) {
  if (cliInventory) {
    return path.resolve(cliInventory);
  }
  return path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
    "references",
    "local-fork-features.md",
  );
}

function readText(filePath) {
  return fs.readFileSync(filePath, "utf8");
}

function writeText(filePath, text) {
  fs.writeFileSync(filePath, text, "utf8");
}

function extractSection(text, startMarker, endMarker) {
  const start = text.indexOf(startMarker);
  const end = text.indexOf(endMarker);
  if (start === -1 || end === -1 || end <= start) {
    throw new Error(`failed to find section ${startMarker} .. ${endMarker}`);
  }
  return text.slice(start + startMarker.length, end).trim();
}

function replaceSection(text, startMarker, endMarker, replacement) {
  const start = text.indexOf(startMarker);
  const end = text.indexOf(endMarker);
  if (start === -1 || end === -1 || end <= start) {
    throw new Error(`failed to replace section ${startMarker} .. ${endMarker}`);
  }
  const body = `\n${replacement.trim()}\n`;
  return `${text.slice(0, start + startMarker.length)}${body}${text.slice(end)}`;
}

function extractJsonBlock(section) {
  const match = section.match(/```json\s*([\s\S]*?)\s*```/m);
  if (!match) {
    throw new Error("failed to find fenced json block in spec section");
  }
  return match[1];
}

function loadSpec(filePath) {
  const raw = readText(filePath);
  const specSection = extractSection(raw, SPEC_START, SPEC_END);
  const spec = JSON.parse(extractJsonBlock(specSection));
  if (!Array.isArray(spec)) {
    throw new Error("spec must be a JSON array");
  }
  return { raw, spec };
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
  const pattern = new RegExp(check.pattern, "ms");
  const match = pattern.exec(text);
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
    if (check.type === "exists") {
      return existsCheck(repoRoot, check);
    }
    throw new Error(`unsupported check type: ${check.type}`);
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

function buildReport(results) {
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

function main() {
  const args = parseArgs(process.argv.slice(2));
  const repoRoot = path.resolve(args.repo);
  const inventoryFile = inventoryPath(args.inventory);
  const { raw, spec } = loadSpec(inventoryFile);
  const results = spec.map((feature) => runFeatureCheck(repoRoot, feature));
  const report = buildReport(results);

  if (args.command === "refresh") {
    const updated = replaceSection(raw, REPORT_START, REPORT_END, report);
    if (updated !== raw) {
      writeText(inventoryFile, updated);
      console.log(`Refreshed ${inventoryFile}`);
    } else {
      console.log(`No changes for ${inventoryFile}`);
    }
  } else {
    console.log(report);
  }

  printFailures(results);
  process.exit(results.every((item) => item.ok) ? 0 : 1);
}

main();
