import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { sanitizeRipgrepConfig } from "../bin/codex.js";

test("sanitizeRipgrepConfig keeps env unchanged when variable is unset", () => {
  const env = { PATH: "/tmp/bin", HOME: "/tmp/home" };

  const sanitized = sanitizeRipgrepConfig(env);

  assert.strictEqual(sanitized, env);
  assert.deepEqual(sanitized, env);
});

test("sanitizeRipgrepConfig removes only invalid RIPGREP_CONFIG_PATH", () => {
  const env = {
    PATH: "/tmp/bin",
    HOME: "/tmp/home",
    RIPGREP_CONFIG_PATH: "/tmp/definitely-missing-ripgreprc",
  };

  const sanitized = sanitizeRipgrepConfig(env);

  assert.equal("RIPGREP_CONFIG_PATH" in sanitized, false);
  assert.equal(sanitized.PATH, env.PATH);
  assert.equal(sanitized.HOME, env.HOME);
});

test("sanitizeRipgrepConfig keeps valid RIPGREP_CONFIG_PATH", () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "codex-rg-config-"));
  const configPath = path.join(tempDir, "ripgreprc");
  fs.writeFileSync(configPath, "--hidden\n");

  const env = {
    PATH: "/tmp/bin",
    HOME: "/tmp/home",
    RIPGREP_CONFIG_PATH: configPath,
  };

  const sanitized = sanitizeRipgrepConfig(env);

  assert.strictEqual(sanitized, env);
  assert.equal(sanitized.RIPGREP_CONFIG_PATH, configPath);
});

test("codex launcher still runs when invoked through a symlink", () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "codex-symlink-"));
  const linkPath = path.join(tempDir, "codex-link.js");
  const scriptPath = path.resolve("bin/codex.js");
  fs.symlinkSync(scriptPath, linkPath);

  const result = spawnSync(process.execPath, [linkPath], {
    cwd: path.resolve("."),
    encoding: "utf8",
    env: { ...process.env, PATH: process.env.PATH || "" },
  });

  assert.notEqual(result.status, 0);
  assert.match(
    `${result.stdout}${result.stderr}`,
    /缺少可选依赖|不支持的 target triple|不支持的平台|Error/,
  );
});
