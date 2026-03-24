#!/usr/bin/env python3
"""安装 Codex 原生二进制（Rust CLI 与 ripgrep 辅助）。"""

import argparse
from contextlib import contextmanager
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
import zipfile
from dataclasses import dataclass
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
import sys
from typing import Iterable, Sequence
from urllib.parse import urlparse
from urllib.request import urlopen

SCRIPT_DIR = Path(__file__).resolve().parent
CODEX_CLI_ROOT = SCRIPT_DIR.parent
DEFAULT_WORKFLOW_REPO = "sohaha/zcodex"
DEFAULT_WORKFLOW_URL = "https://github.com/sohaha/zcodex/actions/runs/23373597239"  # v1.0.0
VENDOR_DIR_NAME = "vendor"
RG_MANIFEST = CODEX_CLI_ROOT / "bin" / "rg"
BINARY_TARGETS = (
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
)


@dataclass(frozen=True)
class BinaryComponent:
    artifact_prefix: str  # 匹配制品文件名前缀（例如 codex-<target>.zst）
    dest_dir: str  # 安装到 vendor/<target>/ 下的目录
    binary_basename: str  # 可执行文件名（不含 .exe）
    targets: tuple[str, ...] | None = None  # 限制仅安装指定 target


WINDOWS_TARGETS = tuple(target for target in BINARY_TARGETS if "windows" in target)

BINARY_COMPONENTS = {
    "codex": BinaryComponent(
        artifact_prefix="codex",
        dest_dir="codex",
        binary_basename="codex",
    ),
    "codex-responses-api-proxy": BinaryComponent(
        artifact_prefix="codex-responses-api-proxy",
        dest_dir="codex-responses-api-proxy",
        binary_basename="codex-responses-api-proxy",
    ),
    "codex-windows-sandbox-setup": BinaryComponent(
        artifact_prefix="codex-windows-sandbox-setup",
        dest_dir="codex",
        binary_basename="codex-windows-sandbox-setup",
        targets=WINDOWS_TARGETS,
    ),
    "codex-command-runner": BinaryComponent(
        artifact_prefix="codex-command-runner",
        dest_dir="codex",
        binary_basename="codex-command-runner",
        targets=WINDOWS_TARGETS,
    ),
}

RG_TARGET_PLATFORM_PAIRS: list[tuple[str, str]] = [
    ("x86_64-unknown-linux-musl", "linux-x86_64"),
    ("aarch64-unknown-linux-musl", "linux-aarch64"),
    ("x86_64-apple-darwin", "macos-x86_64"),
    ("aarch64-apple-darwin", "macos-aarch64"),
    ("x86_64-pc-windows-msvc", "windows-x86_64"),
    ("aarch64-pc-windows-msvc", "windows-aarch64"),
]
RG_TARGET_TO_PLATFORM = {target: platform for target, platform in RG_TARGET_PLATFORM_PAIRS}
DEFAULT_RG_TARGETS = [target for target, _ in RG_TARGET_PLATFORM_PAIRS]

# urllib.request.urlopen() 默认无超时（可能无限阻塞），在 CI 中会很痛苦。
DOWNLOAD_TIMEOUT_SECS = 60


def _gha_enabled() -> bool:
    # GitHub Actions 支持“workflow commands”（如 ::group:: / ::error::），
    # 便于日志阅读：分组可折叠噪声，错误注释会在 UI 中突出显示，
    # 且不会改变实际异常/回溯输出。
    return os.environ.get("GITHUB_ACTIONS") == "true"


def _gha_escape(value: str) -> str:
    # workflow commands 需要对 % 和换行进行转义。
    return value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def _gha_error(*, title: str, message: str) -> None:
    # 输出 GitHub Actions 错误注释。不会替代 stdout/stderr，只在作业 UI 中添加醒目摘要，
    # 便于定位根因。
    if not _gha_enabled():
        return
    print(
        f"::error title={_gha_escape(title)}::{_gha_escape(message)}",
        flush=True,
    )


@contextmanager
def _gha_group(title: str):
    # 在 GitHub Actions 中将日志包裹为可折叠分组；本地运行则不做处理。
    if _gha_enabled():
        print(f"::group::{_gha_escape(title)}", flush=True)
    try:
        yield
    finally:
        if _gha_enabled():
            print("::endgroup::", flush=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="安装 Codex 原生二进制。")
    parser.add_argument(
        "--workflow-url",
        help=(
            "生成制品的 GitHub Actions 工作流 URL。未提供时使用已验证的默认运行。"
        ),
    )
    parser.add_argument(
        "--component",
        dest="components",
        action="append",
        choices=tuple(list(BINARY_COMPONENTS) + ["rg"]),
        help=(
            "仅安装指定组件。可重复传入。默认包含 codex、codex-windows-sandbox-setup、"
            "codex-command-runner 与 rg。"
        ),
    )
    parser.add_argument(
        "root",
        nargs="?",
        type=Path,
        help=(
            "包含 package.json 的暂存目录。未提供时使用仓库目录。"
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    codex_cli_root = (args.root or CODEX_CLI_ROOT).resolve()
    vendor_dir = codex_cli_root / VENDOR_DIR_NAME
    vendor_dir.mkdir(parents=True, exist_ok=True)

    components = args.components or [
        "codex",
        "codex-windows-sandbox-setup",
        "codex-command-runner",
        "rg",
    ]

    workflow_url = (args.workflow_url or DEFAULT_WORKFLOW_URL).strip()
    if not workflow_url:
        workflow_url = DEFAULT_WORKFLOW_URL

    workflow_repo, workflow_id = resolve_workflow_run(workflow_url)
    print(f"正在从工作流 {workflow_repo}#{workflow_id} 下载原生制品...")

    with _gha_group(f"下载工作流 {workflow_repo}#{workflow_id} 的原生制品"):
        with tempfile.TemporaryDirectory(prefix="codex-native-artifacts-") as artifacts_dir_str:
            artifacts_dir = Path(artifacts_dir_str)
            _download_artifacts(workflow_repo, workflow_id, artifacts_dir)
            install_binary_components(
                artifacts_dir,
                vendor_dir,
                [BINARY_COMPONENTS[name] for name in components if name in BINARY_COMPONENTS],
            )

    if "rg" in components:
        with _gha_group("获取 ripgrep 二进制"):
            print("正在获取 ripgrep 二进制...")
            fetch_rg(vendor_dir, DEFAULT_RG_TARGETS, manifest_path=RG_MANIFEST)

    print(f"已安装原生依赖到 {vendor_dir}")
    return 0


def fetch_rg(
    vendor_dir: Path,
    targets: Sequence[str] | None = None,
    *,
    manifest_path: Path,
) -> list[Path]:
    """下载 DotSlash manifest 中描述的 ripgrep 二进制。"""

    if targets is None:
        targets = DEFAULT_RG_TARGETS

    if not manifest_path.exists():
        raise FileNotFoundError(f"未找到 DotSlash manifest：{manifest_path}")

    manifest = _load_manifest(manifest_path)
    platforms = manifest.get("platforms", {})

    vendor_dir.mkdir(parents=True, exist_ok=True)

    targets = list(targets)
    if not targets:
        return []

    task_configs: list[tuple[str, str, dict]] = []
    for target in targets:
        platform_key = RG_TARGET_TO_PLATFORM.get(target)
        if platform_key is None:
            raise ValueError(f"不支持的 ripgrep target：'{target}'。")

        platform_info = platforms.get(platform_key)
        if platform_info is None:
            raise RuntimeError(f"在 manifest {manifest_path} 中找不到平台 '{platform_key}'。")

        task_configs.append((target, platform_key, platform_info))

    results: dict[str, Path] = {}
    max_workers = min(len(task_configs), max(1, (os.cpu_count() or 1)))

    print("正在安装 ripgrep 二进制，目标： " + ", ".join(targets))

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        future_map = {
            executor.submit(
                _fetch_single_rg,
                vendor_dir,
                target,
                platform_key,
                platform_info,
                manifest_path,
            ): target
            for target, platform_key, platform_info in task_configs
        }

        for future in as_completed(future_map):
            target = future_map[future]
            try:
                results[target] = future.result()
            except Exception as exc:
                _gha_error(
                    title="ripgrep 安装失败",
                    message=f"target={target} error={exc!r}",
                )
                raise RuntimeError(f"安装 ripgrep 失败，目标 {target}。") from exc
            print(f"  已安装 ripgrep：{target}")

    return [results[target] for target in targets]


def resolve_workflow_run(workflow_url: str) -> tuple[str, str]:
    parsed = urlparse(workflow_url)
    if parsed.scheme and parsed.netloc:
        parts = [part for part in parsed.path.split("/") if part]
        if len(parts) >= 4 and parts[2] == "actions" and parts[3] == "runs":
            workflow_id = parts[4] if len(parts) >= 5 else ""
            if workflow_id:
                return f"{parts[0]}/{parts[1]}", workflow_id
        raise ValueError(f"不支持的 GitHub Actions 工作流 URL：{workflow_url}")

    workflow_id = workflow_url.strip().rstrip("/")
    if workflow_id.isdigit():
        return DEFAULT_WORKFLOW_REPO, workflow_id

    raise ValueError(f"不支持的工作流引用：{workflow_url}")


def _download_artifacts(repo: str, workflow_id: str, dest_dir: Path) -> None:
    cmd = [
        "gh",
        "run",
        "download",
        "--dir",
        str(dest_dir),
        "--repo",
        repo,
        workflow_id,
    ]
    subprocess.check_call(cmd)


def install_binary_components(
    artifacts_dir: Path,
    vendor_dir: Path,
    selected_components: Sequence[BinaryComponent],
) -> None:
    if not selected_components:
        return

    for component in selected_components:
        component_targets = list(component.targets or BINARY_TARGETS)

        print(
            f"正在安装 {component.binary_basename} 二进制，目标： "
            + ", ".join(component_targets)
        )
        max_workers = min(len(component_targets), max(1, (os.cpu_count() or 1)))
        with ThreadPoolExecutor(max_workers=max_workers) as executor:
            futures = {
                executor.submit(
                    _install_single_binary,
                    artifacts_dir,
                    vendor_dir,
                    target,
                    component,
                ): target
                for target in component_targets
            }
            for future in as_completed(futures):
                installed_path = future.result()
                print(f"  已安装 {installed_path}")


def _install_single_binary(
    artifacts_dir: Path,
    vendor_dir: Path,
    target: str,
    component: BinaryComponent,
) -> Path:
    artifact_subdir = artifacts_dir / target
    archive_name = _archive_name_for_target(component.artifact_prefix, target)
    archive_path = artifact_subdir / archive_name
    if not archive_path.exists():
        raise FileNotFoundError(f"未找到预期制品：{archive_path}")

    dest_dir = vendor_dir / target / component.dest_dir
    dest_dir.mkdir(parents=True, exist_ok=True)

    binary_name = (
        f"{component.binary_basename}.exe" if "windows" in target else component.binary_basename
    )
    dest = dest_dir / binary_name
    dest.unlink(missing_ok=True)
    extract_archive(archive_path, "zst", None, dest)
    if "windows" not in target:
        dest.chmod(0o755)
    return dest


def _archive_name_for_target(artifact_prefix: str, target: str) -> str:
    if "windows" in target:
        return f"{artifact_prefix}-{target}.exe.zst"
    return f"{artifact_prefix}-{target}.zst"


def _fetch_single_rg(
    vendor_dir: Path,
    target: str,
    platform_key: str,
    platform_info: dict,
    manifest_path: Path,
) -> Path:
    providers = platform_info.get("providers", [])
    if not providers:
        raise RuntimeError(f"manifest {manifest_path} 中未列出平台 '{platform_key}' 的 providers。")

    url = providers[0]["url"]
    archive_format = platform_info.get("format", "zst")
    archive_member = platform_info.get("path")
    digest = platform_info.get("digest")
    expected_size = platform_info.get("size")

    dest_dir = vendor_dir / target / "path"
    dest_dir.mkdir(parents=True, exist_ok=True)

    is_windows = platform_key.startswith("win")
    binary_name = "rg.exe" if is_windows else "rg"
    dest = dest_dir / binary_name

    with tempfile.TemporaryDirectory() as tmp_dir_str:
        tmp_dir = Path(tmp_dir_str)
        archive_filename = os.path.basename(urlparse(url).path)
        download_path = tmp_dir / archive_filename
        print(
            f"  正在下载 ripgrep：{target}（{platform_key}），来源 {url}",
            flush=True,
        )
        try:
            _download_file(url, download_path)
        except Exception as exc:
            _gha_error(
                title="ripgrep 下载失败",
                message=f"target={target} platform={platform_key} url={url} error={exc!r}",
            )
            raise RuntimeError(
                "下载 ripgrep 失败 "
                f"(target={target}, platform={platform_key}, format={archive_format}, "
                f"expected_size={expected_size!r}, digest={digest!r}, url={url}, dest={download_path})."
            ) from exc

        dest.unlink(missing_ok=True)
        try:
            extract_archive(download_path, archive_format, archive_member, dest)
        except Exception as exc:
            raise RuntimeError(
                "解压 ripgrep 失败 "
                f"(target={target}, platform={platform_key}, format={archive_format}, "
                f"member={archive_member!r}, url={url}, archive={download_path})."
            ) from exc

    if not is_windows:
        dest.chmod(0o755)

    return dest


def _download_file(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.unlink(missing_ok=True)

    with urlopen(url, timeout=DOWNLOAD_TIMEOUT_SECS) as response, open(dest, "wb") as out:
        shutil.copyfileobj(response, out)


def extract_archive(
    archive_path: Path,
    archive_format: str,
    archive_member: str | None,
    dest: Path,
) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)

    if archive_format == "zst":
        output_path = archive_path.parent / dest.name
        subprocess.check_call(
            ["zstd", "-f", "-d", str(archive_path), "-o", str(output_path)]
        )
        shutil.move(str(output_path), dest)
        return

    if archive_format == "tar.gz":
        if not archive_member:
            raise RuntimeError("DotSlash manifest 的 tar.gz 归档缺少 'path'。")
        with tarfile.open(archive_path, "r:gz") as tar:
            try:
                member = tar.getmember(archive_member)
            except KeyError as exc:
                raise RuntimeError(
                    f"归档 {archive_path} 中找不到条目 '{archive_member}'。"
                ) from exc
            tar.extract(member, path=archive_path.parent, filter="data")
        extracted = archive_path.parent / archive_member
        shutil.move(str(extracted), dest)
        return

    if archive_format == "zip":
        if not archive_member:
            raise RuntimeError("DotSlash manifest 的 zip 归档缺少 'path'。")
        with zipfile.ZipFile(archive_path) as archive:
            try:
                with archive.open(archive_member) as src, open(dest, "wb") as out:
                    shutil.copyfileobj(src, out)
            except KeyError as exc:
                raise RuntimeError(
                    f"归档 {archive_path} 中找不到条目 '{archive_member}'。"
                ) from exc
        return

    raise RuntimeError(f"不支持的归档格式 '{archive_format}'。")


def _load_manifest(manifest_path: Path) -> dict:
    cmd = ["dotslash", "--", "parse", str(manifest_path)]
    stdout = subprocess.check_output(cmd, text=True)
    try:
        manifest = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"DotSlash manifest 输出无效：{manifest_path}。") from exc

    if not isinstance(manifest, dict):
        raise RuntimeError(
            f"DotSlash manifest 结构异常：{manifest_path}，类型 {type(manifest)!r}"
        )

    return manifest


if __name__ == "__main__":
    import sys

    sys.exit(main())
