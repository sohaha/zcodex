#!/usr/bin/env python3
"""Check key fork config items for local operations readiness.

Usage:
  scripts/check-fork-config.py
  scripts/check-fork-config.py --config ~/.codex/config.toml --strict
"""

from __future__ import annotations

import argparse
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError as exc:  # pragma: no cover
    raise SystemExit("python3.11+ is required (missing tomllib)") from exc


@dataclass
class Item:
    level: str
    message: str


def _is_non_empty_string(value: Any) -> bool:
    return isinstance(value, str) and value.strip() != ""


def _check_provider(config: dict[str, Any], items: list[Item]) -> None:
    provider = config.get("model_provider")
    model = config.get("model")
    providers = config.get("model_providers")

    if _is_non_empty_string(provider):
        items.append(Item("PASS", f"model_provider is set: {provider!r}"))
    else:
        items.append(Item("WARN", "model_provider is not explicitly set"))

    if _is_non_empty_string(model):
        items.append(Item("PASS", f"model is set: {model!r}"))
    else:
        items.append(Item("WARN", "model is not explicitly set"))

    if not isinstance(providers, dict):
        items.append(Item("WARN", "[model_providers] table is missing"))
        return

    if _is_non_empty_string(provider):
        provider_cfg = providers.get(provider)
        if isinstance(provider_cfg, dict):
            items.append(Item("PASS", f"[model_providers.{provider}] exists"))
            env_key = provider_cfg.get("env_key")
            if _is_non_empty_string(env_key):
                items.append(Item("PASS", f"{provider} env_key is set: {env_key!r}"))
                if os.getenv(env_key):
                    items.append(Item("PASS", f"environment variable {env_key} is available"))
                else:
                    items.append(
                        Item(
                            "WARN",
                            f"environment variable {env_key} is not set in current shell",
                        )
                    )
            else:
                items.append(Item("WARN", f"{provider} env_key is not set"))
        else:
            items.append(
                Item("WARN", f"model_provider={provider!r} but [model_providers.{provider}] is missing")
            )


def _check_cron(config: dict[str, Any], items: list[Item]) -> None:
    disable_cron = config.get("disable_cron")
    if isinstance(disable_cron, bool):
        if disable_cron:
            items.append(Item("WARN", "disable_cron=true, /loop and scheduled tasks are disabled"))
        else:
            items.append(Item("PASS", "disable_cron=false, /loop and scheduled tasks are enabled"))
    else:
        items.append(Item("WARN", "disable_cron is not explicitly set"))


def _check_hooks(config: dict[str, Any], items: list[Item]) -> None:
    hooks = config.get("hooks")
    if not isinstance(hooks, dict):
        items.append(Item("WARN", "[hooks] is missing"))
        return

    pre_tool_use = hooks.get("pre_tool_use")
    if isinstance(pre_tool_use, list) and pre_tool_use:
        items.append(Item("PASS", f"hooks.pre_tool_use has {len(pre_tool_use)} rule(s)"))
    else:
        items.append(Item("WARN", "hooks.pre_tool_use is empty (no shell/exec guardrails found)"))


def _check_webhook(config: dict[str, Any], items: list[Item]) -> None:
    webhook = config.get("github_webhook")
    if not isinstance(webhook, dict):
        items.append(Item("WARN", "[github_webhook] is missing"))
        return

    items.append(Item("PASS", "[github_webhook] section exists"))
    enabled = webhook.get("enabled")
    if enabled is True:
        items.append(Item("PASS", "github_webhook.enabled=true"))
    else:
        items.append(Item("WARN", "github_webhook.enabled is not true"))
        return

    required_string_keys = ["listen", "command_prefix", "webhook_secret_env"]
    for key in required_string_keys:
        value = webhook.get(key)
        if _is_non_empty_string(value):
            items.append(Item("PASS", f"github_webhook.{key} is set"))
        else:
            items.append(Item("WARN", f"github_webhook.{key} is missing"))

    auth_keys = ["github_token_env", "github_app_id_env", "github_app_private_key_env"]
    auth_values = [webhook.get(k) for k in auth_keys if _is_non_empty_string(webhook.get(k))]
    if auth_values:
        items.append(Item("PASS", "github_webhook auth env key(s) are configured"))
    else:
        items.append(
            Item(
                "WARN",
                "github_webhook has no auth env key configured "
                "(github_token_env or github_app_*_env)",
            )
        )

    for key in ["webhook_secret_env", *auth_keys]:
        env_name = webhook.get(key)
        if _is_non_empty_string(env_name):
            if os.getenv(env_name):
                items.append(Item("PASS", f"environment variable {env_name} is available"))
            else:
                items.append(
                    Item("WARN", f"environment variable {env_name} is not set in current shell")
                )


def run(config_path: Path, strict: bool) -> int:
    if not config_path.exists():
        print(f"FAIL: config file not found: {config_path}")
        return 2

    try:
        content = config_path.read_bytes()
        config = tomllib.loads(content.decode("utf-8"))
    except Exception as exc:
        print(f"FAIL: failed to parse TOML: {exc}")
        return 2

    if not isinstance(config, dict):
        print("FAIL: parsed config is not a TOML table")
        return 2

    items: list[Item] = []
    _check_provider(config, items)
    _check_cron(config, items)
    _check_hooks(config, items)
    _check_webhook(config, items)

    warn_count = 0
    for item in items:
        print(f"{item.level}: {item.message}")
        if item.level == "WARN":
            warn_count += 1

    if warn_count == 0:
        print("RESULT: PASS")
        return 0

    if strict:
        print(f"RESULT: FAIL ({warn_count} warning(s), strict mode)")
        return 1

    print(f"RESULT: WARN ({warn_count} warning(s))")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Check key fork config items.")
    parser.add_argument(
        "--config",
        default="~/.codex/config.toml",
        help="Path to config.toml (default: ~/.codex/config.toml)",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Return non-zero exit code when WARN items exist",
    )
    args = parser.parse_args()
    path = Path(args.config).expanduser()
    return run(path, strict=args.strict)


if __name__ == "__main__":
    raise SystemExit(main())
