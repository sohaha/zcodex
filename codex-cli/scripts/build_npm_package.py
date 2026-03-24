#!/usr/bin/env python3
"""暂存并可选打包 @sohaha/zcodex npm 模块。"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
CODEX_CLI_ROOT = SCRIPT_DIR.parent
REPO_ROOT = CODEX_CLI_ROOT.parent
RESPONSES_API_PROXY_NPM_ROOT = REPO_ROOT / "codex-rs" / "responses-api-proxy" / "npm"
CODEX_SDK_ROOT = REPO_ROOT / "sdk" / "typescript"
CODEX_NPM_NAME = "@sohaha/zcodex"

# `npm_name` 是 `bin/codex.js` 使用的本地 optionalDependencies 别名。
# 实际发布到 npm 的包始终是 `@sohaha/zcodex`。
CODEX_PLATFORM_PACKAGES: dict[str, dict[str, str]] = {
    "codex-linux-x64": {
        "npm_name": "@sohaha/zcodex-linux-x64",
        "npm_tag": "linux-x64",
        "target_triple": "x86_64-unknown-linux-musl",
        "os": "linux",
        "cpu": "x64",
    },
    "codex-linux-arm64": {
        "npm_name": "@sohaha/zcodex-linux-arm64",
        "npm_tag": "linux-arm64",
        "target_triple": "aarch64-unknown-linux-musl",
        "os": "linux",
        "cpu": "arm64",
    },
    "codex-darwin-x64": {
        "npm_name": "@sohaha/zcodex-darwin-x64",
        "npm_tag": "darwin-x64",
        "target_triple": "x86_64-apple-darwin",
        "os": "darwin",
        "cpu": "x64",
    },
    "codex-darwin-arm64": {
        "npm_name": "@sohaha/zcodex-darwin-arm64",
        "npm_tag": "darwin-arm64",
        "target_triple": "aarch64-apple-darwin",
        "os": "darwin",
        "cpu": "arm64",
    },
    "codex-win32-x64": {
        "npm_name": "@sohaha/zcodex-win32-x64",
        "npm_tag": "win32-x64",
        "target_triple": "x86_64-pc-windows-msvc",
        "os": "win32",
        "cpu": "x64",
    },
    "codex-win32-arm64": {
        "npm_name": "@sohaha/zcodex-win32-arm64",
        "npm_tag": "win32-arm64",
        "target_triple": "aarch64-pc-windows-msvc",
        "os": "win32",
        "cpu": "arm64",
    },
}

PACKAGE_EXPANSIONS: dict[str, list[str]] = {
    "codex": ["codex", *CODEX_PLATFORM_PACKAGES],
}

PACKAGE_NATIVE_COMPONENTS: dict[str, list[str]] = {
    "codex": [],
    "codex-linux-x64": ["codex", "rg"],
    "codex-linux-arm64": ["codex", "rg"],
    "codex-darwin-x64": ["codex", "rg"],
    "codex-darwin-arm64": ["codex", "rg"],
    "codex-win32-x64": ["codex", "rg", "codex-windows-sandbox-setup", "codex-command-runner"],
    "codex-win32-arm64": ["codex", "rg", "codex-windows-sandbox-setup", "codex-command-runner"],
    "codex-responses-api-proxy": ["codex-responses-api-proxy"],
    "codex-sdk": [],
}

PACKAGE_TARGET_FILTERS: dict[str, str] = {
    package_name: package_config["target_triple"]
    for package_name, package_config in CODEX_PLATFORM_PACKAGES.items()
}

PACKAGE_CHOICES = tuple(PACKAGE_NATIVE_COMPONENTS)

COMPONENT_DEST_DIR: dict[str, str] = {
    "codex": "codex",
    "codex-responses-api-proxy": "codex-responses-api-proxy",
    "codex-windows-sandbox-setup": "codex",
    "codex-command-runner": "codex",
    "rg": "path",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="构建或暂存 Codex CLI npm 包。")
    parser.add_argument(
        "--package",
        choices=PACKAGE_CHOICES,
        default="codex",
        help="要暂存的 npm 包（默认：codex）。",
    )
    parser.add_argument(
        "--version",
        help="写入暂存 package.json 的版本号。",
    )
    parser.add_argument(
        "--release-version",
        help=(
            "用于 npm 发布的暂存版本号。"
        ),
    )
    parser.add_argument(
        "--staging-dir",
        type=Path,
        help=(
            "用于暂存包内容的目录。未指定则使用新的临时目录。指定目录时必须为空。"
        ),
    )
    parser.add_argument(
        "--tmp",
        dest="staging_dir",
        type=Path,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--pack-output",
        type=Path,
        help="生成的 npm tarball 输出路径。",
    )
    parser.add_argument(
        "--vendor-src",
        type=Path,
        help="包含预安装原生二进制的目录（vendor 根目录）。",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    package = args.package
    version = args.version
    release_version = args.release_version
    if release_version:
        if version and version != release_version:
            raise RuntimeError("同时提供 --version 与 --release-version 时必须一致。")
        version = release_version

    if not version:
        raise RuntimeError("必须指定 --version 或 --release-version。")

    staging_dir, created_temp = prepare_staging_dir(args.staging_dir)

    try:
        stage_sources(staging_dir, version, package)

        vendor_src = args.vendor_src.resolve() if args.vendor_src else None
        native_components = PACKAGE_NATIVE_COMPONENTS.get(package, [])
        target_filter = PACKAGE_TARGET_FILTERS.get(package)

        if native_components:
            if vendor_src is None:
                components_str = ", ".join(native_components)
                raise RuntimeError(
                    "该包需要原生组件 "
                    f"({components_str})，请为包 '{package}' 提供 --vendor-src，"
                    "指向包含预安装二进制的目录。"
                )

            copy_native_binaries(
                vendor_src,
                staging_dir,
                native_components,
                target_filter={target_filter} if target_filter else None,
            )

        if release_version:
            staging_dir_str = str(staging_dir)
            if package == "codex":
                print(
                    f"已在 {staging_dir_str} 暂存用于发布的版本 {version}\n\n"
                    "验证 CLI：\n"
                    f"    node {staging_dir_str}/bin/codex.js --version\n"
                    f"    node {staging_dir_str}/bin/codex.js --help\n\n"
                )
            elif package == "codex-responses-api-proxy":
                print(
                    f"已在 {staging_dir_str} 暂存用于发布的版本 {version}\n\n"
                    "验证 responses API 代理：\n"
                    f"    node {staging_dir_str}/bin/codex-responses-api-proxy.js --help\n\n"
                )
            elif package in CODEX_PLATFORM_PACKAGES:
                print(
                    f"已在 {staging_dir_str} 暂存用于发布的版本 {version}\n\n"
                    "验证原生文件内容：\n"
                    f"    ls {staging_dir_str}/vendor\n\n"
                )
            else:
                print(
                    f"已在 {staging_dir_str} 暂存用于发布的版本 {version}\n\n"
                    "验证 SDK 内容：\n"
                    f"    ls {staging_dir_str}/dist\n"
                    "    node -e \"import('./dist/index.js').then(() => console.log('ok'))\"\n\n"
                )
        else:
            print(f"已在 {staging_dir} 暂存包内容")

        if args.pack_output is not None:
            output_path = run_npm_pack(staging_dir, args.pack_output)
            print(f"npm pack 输出已写入 {output_path}")
    finally:
        if created_temp:
            # 保留暂存目录以便后续检查。
            pass

    return 0


def prepare_staging_dir(staging_dir: Path | None) -> tuple[Path, bool]:
    if staging_dir is not None:
        staging_dir = staging_dir.resolve()
        staging_dir.mkdir(parents=True, exist_ok=True)
        if any(staging_dir.iterdir()):
            raise RuntimeError(f"暂存目录 {staging_dir} 不为空。")
        return staging_dir, False

    temp_dir = Path(tempfile.mkdtemp(prefix="codex-npm-stage-"))
    return temp_dir, True


def stage_sources(staging_dir: Path, version: str, package: str) -> None:
    package_json: dict
    package_json_path: Path | None = None

    if package == "codex":
        bin_dir = staging_dir / "bin"
        bin_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(CODEX_CLI_ROOT / "bin" / "codex.js", bin_dir / "codex.js")
        rg_manifest = CODEX_CLI_ROOT / "bin" / "rg"
        if rg_manifest.exists():
            shutil.copy2(rg_manifest, bin_dir / "rg")

        readme_src = REPO_ROOT / "README.md"
        if readme_src.exists():
            shutil.copy2(readme_src, staging_dir / "README.md")

        package_json_path = CODEX_CLI_ROOT / "package.json"
    elif package in CODEX_PLATFORM_PACKAGES:
        platform_package = CODEX_PLATFORM_PACKAGES[package]
        platform_npm_tag = platform_package["npm_tag"]
        platform_version = compute_platform_package_version(version, platform_npm_tag)

        readme_src = REPO_ROOT / "README.md"
        if readme_src.exists():
            shutil.copy2(readme_src, staging_dir / "README.md")

        with open(CODEX_CLI_ROOT / "package.json", "r", encoding="utf-8") as fh:
            codex_package_json = json.load(fh)

        package_json = {
            "name": CODEX_NPM_NAME,
            "version": platform_version,
            "license": codex_package_json.get("license", "Apache-2.0"),
            "os": [platform_package["os"]],
            "cpu": [platform_package["cpu"]],
            "files": ["vendor"],
            "repository": codex_package_json.get("repository"),
        }

        engines = codex_package_json.get("engines")
        if isinstance(engines, dict):
            package_json["engines"] = engines

        package_manager = codex_package_json.get("packageManager")
        if isinstance(package_manager, str):
            package_json["packageManager"] = package_manager
    elif package == "codex-responses-api-proxy":
        bin_dir = staging_dir / "bin"
        bin_dir.mkdir(parents=True, exist_ok=True)
        launcher_src = RESPONSES_API_PROXY_NPM_ROOT / "bin" / "codex-responses-api-proxy.js"
        shutil.copy2(launcher_src, bin_dir / "codex-responses-api-proxy.js")

        readme_src = RESPONSES_API_PROXY_NPM_ROOT / "README.md"
        if readme_src.exists():
            shutil.copy2(readme_src, staging_dir / "README.md")

        package_json_path = RESPONSES_API_PROXY_NPM_ROOT / "package.json"
    elif package == "codex-sdk":
        package_json_path = CODEX_SDK_ROOT / "package.json"
        stage_codex_sdk_sources(staging_dir)
    else:
        raise RuntimeError(f"未知的包 '{package}'。")

    if package_json_path is not None:
        with open(package_json_path, "r", encoding="utf-8") as fh:
            package_json = json.load(fh)
        package_json["version"] = version

    if package == "codex":
        package_json["files"] = ["bin"]
        package_json["optionalDependencies"] = {
            CODEX_PLATFORM_PACKAGES[platform_package]["npm_name"]: (
                f"npm:{CODEX_NPM_NAME}@"
                f"{compute_platform_package_version(version, CODEX_PLATFORM_PACKAGES[platform_package]['npm_tag'])}"
            )
            for platform_package in PACKAGE_EXPANSIONS["codex"]
            if platform_package != "codex"
        }

    elif package == "codex-sdk":
        scripts = package_json.get("scripts")
        if isinstance(scripts, dict):
            scripts.pop("prepare", None)

        dependencies = package_json.get("dependencies")
        if not isinstance(dependencies, dict):
            dependencies = {}
        dependencies[CODEX_NPM_NAME] = version
        package_json["dependencies"] = dependencies

    with open(staging_dir / "package.json", "w", encoding="utf-8") as out:
        json.dump(package_json, out, indent=2)
        out.write("\n")


def compute_platform_package_version(version: str, platform_tag: str) -> str:
    # npm 禁止重复发布相同包名/版本，因此每个平台的 tarball 都需要唯一版本号。
    return f"{version}-{platform_tag}"


def run_command(cmd: list[str], cwd: Path | None = None) -> None:
    print("+", " ".join(cmd))
    subprocess.run(cmd, cwd=cwd, check=True)


def stage_codex_sdk_sources(staging_dir: Path) -> None:
    package_root = CODEX_SDK_ROOT

    run_command(["pnpm", "install", "--frozen-lockfile"], cwd=package_root)
    run_command(["pnpm", "run", "build"], cwd=package_root)

    dist_src = package_root / "dist"
    if not dist_src.exists():
        raise RuntimeError("codex-sdk 构建未生成 dist 目录。")

    shutil.copytree(dist_src, staging_dir / "dist")

    readme_src = package_root / "README.md"
    if readme_src.exists():
        shutil.copy2(readme_src, staging_dir / "README.md")

    license_src = REPO_ROOT / "LICENSE"
    if license_src.exists():
        shutil.copy2(license_src, staging_dir / "LICENSE")


def copy_native_binaries(
    vendor_src: Path,
    staging_dir: Path,
    components: list[str],
    target_filter: set[str] | None = None,
) -> None:
    vendor_src = vendor_src.resolve()
    if not vendor_src.exists():
        raise RuntimeError(f"未找到 vendor 源目录：{vendor_src}")

    components_set = {component for component in components if component in COMPONENT_DEST_DIR}
    if not components_set:
        return

    vendor_dest = staging_dir / "vendor"
    if vendor_dest.exists():
        shutil.rmtree(vendor_dest)
    vendor_dest.mkdir(parents=True, exist_ok=True)

    copied_targets: set[str] = set()

    for target_dir in vendor_src.iterdir():
        if not target_dir.is_dir():
            continue

        if target_filter is not None and target_dir.name not in target_filter:
            continue

        dest_target_dir = vendor_dest / target_dir.name
        dest_target_dir.mkdir(parents=True, exist_ok=True)
        copied_targets.add(target_dir.name)

        for component in components_set:
            dest_dir_name = COMPONENT_DEST_DIR.get(component)
            if dest_dir_name is None:
                continue

            src_component_dir = target_dir / dest_dir_name
            if not src_component_dir.exists():
                raise RuntimeError(
                    f"vendor 源中缺少原生组件 '{component}'：{src_component_dir}"
                )

            dest_component_dir = dest_target_dir / dest_dir_name
            if dest_component_dir.exists():
                shutil.rmtree(dest_component_dir)
            shutil.copytree(src_component_dir, dest_component_dir)

    if target_filter is not None:
        missing_targets = sorted(target_filter - copied_targets)
        if missing_targets:
            missing_list = ", ".join(missing_targets)
            raise RuntimeError(f"vendor 源中缺少目标目录：{missing_list}")


def run_npm_pack(staging_dir: Path, output_path: Path) -> Path:
    output_path = output_path.resolve()
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="codex-npm-pack-") as pack_dir_str:
        pack_dir = Path(pack_dir_str)
        stdout = subprocess.check_output(
            ["npm", "pack", "--json", "--pack-destination", str(pack_dir)],
            cwd=staging_dir,
            text=True,
        )
        try:
            pack_output = json.loads(stdout)
        except json.JSONDecodeError as exc:
            raise RuntimeError("解析 npm pack 输出失败。") from exc

        if not pack_output:
            raise RuntimeError("npm pack 未生成输出 tarball。")

        tarball_name = pack_output[0].get("filename") or pack_output[0].get("name")
        if not tarball_name:
            raise RuntimeError("无法确定 npm pack 输出文件名。")

        tarball_path = pack_dir / tarball_name
        if not tarball_path.exists():
            raise RuntimeError(f"未找到预期的 npm pack 输出：{tarball_path}")

        shutil.move(str(tarball_path), output_path)

    return output_path


if __name__ == "__main__":
    import sys

    sys.exit(main())
