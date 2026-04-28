use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use codex_native_tldr::semantic::SemanticConfig;
use codex_native_tldr::semantic::warm_embedding_model;
#[cfg(unix)]
use std::ffi::CStr;
#[cfg(unix)]
use std::ffi::CString;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use tempfile::Builder;
use tokio::process::Command;

const ONNX_RUNTIME_VERSION: &str = "1.23.2";

#[derive(Debug, Parser)]
pub struct ZinitCli {
    /// 只检测环境，不执行安装。
    #[arg(long, default_value_t = false)]
    check: bool,

    /// 安装 ONNX Runtime 动态库的目录；默认使用当前 codex 可执行文件所在目录。
    #[arg(long = "target-dir", value_name = "目录")]
    target_dir: Option<PathBuf>,

    /// 预下载并初始化指定的 ztldr embedding 模型；不传则只准备 ONNX Runtime。
    #[arg(long = "model", value_name = "模型")]
    model: Option<String>,

    #[command(subcommand)]
    subcommand: Option<ZinitSubcommand>,
}

#[derive(Debug, Subcommand)]
enum ZinitSubcommand {
    /// 初始化 ztldr 语义检索所需的 ONNX Runtime 动态库。
    Ztldr,
}

pub async fn run_zinit_command(cli: ZinitCli) -> Result<()> {
    match cli.subcommand.unwrap_or(ZinitSubcommand::Ztldr) {
        ZinitSubcommand::Ztldr => run_ztldr_init(cli.check, cli.target_dir, cli.model).await,
    }
}

async fn run_ztldr_init(
    check_only: bool,
    target_dir: Option<PathBuf>,
    model: Option<String>,
) -> Result<()> {
    let install_target = resolve_install_target(target_dir)?;
    let target_dir = install_target
        .dylib_path
        .parent()
        .map(Path::to_path_buf)
        .context("ONNX Runtime 动态库目标路径没有父目录")?;
    let dylib_path = install_target.dylib_path;
    let check = check_onnxruntime(&dylib_path);
    if check.ready {
        println!(
            "ztldr 环境已就绪：{}",
            check.path.as_deref().unwrap_or(&dylib_path).display()
        );
        warm_optional_model(model)?;
        return Ok(());
    }

    if check_only {
        bail!(
            "ztldr 环境缺少 ONNX Runtime 动态库：{}",
            check
                .reason
                .unwrap_or_else(|| dylib_path.display().to_string())
        );
    }

    let package = OnnxRuntimePackage::for_current_platform()?;
    println!(
        "ztldr 环境缺少 ONNX Runtime，正在安装 {}...",
        package.asset_name
    );
    install_onnxruntime(package, &target_dir).await?;

    let check = check_onnxruntime(&dylib_path);
    if check.ready {
        println!("ztldr 环境已就绪：{}", dylib_path.display());
        warm_optional_model(model)?;
        return Ok(());
    }

    bail!(
        "ONNX Runtime 已安装但仍不可加载：{}",
        check
            .reason
            .unwrap_or_else(|| dylib_path.display().to_string())
    );
}

fn warm_optional_model(model: Option<String>) -> Result<()> {
    let Some(model) = model else {
        return Ok(());
    };
    let dimensions = SemanticConfig::default().embedding_dimensions();
    println!("正在预热 ztldr embedding 模型：{model}");
    warm_embedding_model(&model, dimensions)?;
    println!("ztldr embedding 模型已就绪：{model}");
    Ok(())
}

#[derive(Debug)]
struct InstallTarget {
    dylib_path: PathBuf,
}

fn resolve_install_target(target_dir: Option<PathBuf>) -> Result<InstallTarget> {
    resolve_install_target_with(
        target_dir,
        std::env::var_os("ORT_DYLIB_PATH"),
        default_install_dir,
    )
}

fn resolve_install_target_with(
    target_dir: Option<PathBuf>,
    ort_dylib_path: Option<std::ffi::OsString>,
    default_install_dir: impl FnOnce() -> Result<PathBuf>,
) -> Result<InstallTarget> {
    if let Some(path) = ort_dylib_path
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        if target_dir.is_some() {
            eprintln!(
                "检测到 ORT_DYLIB_PATH={}，将优先修复该路径。",
                path.display()
            );
        }
        return Ok(InstallTarget { dylib_path: path });
    }

    let target_dir = match target_dir {
        Some(dir) => dir,
        None => default_install_dir()?,
    };
    Ok(InstallTarget {
        dylib_path: target_dir.join(default_onnxruntime_dylib_name()),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Tgz,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OnnxRuntimePackage {
    asset_name: &'static str,
    archive_kind: ArchiveKind,
}

impl OnnxRuntimePackage {
    fn for_current_platform() -> Result<Self> {
        package_for_platform(std::env::consts::OS, std::env::consts::ARCH).with_context(|| {
            format!(
                "当前平台不支持自动安装 ONNX Runtime：{}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            )
        })
    }

    fn url(&self) -> String {
        format!(
            "https://github.com/microsoft/onnxruntime/releases/download/v{ONNX_RUNTIME_VERSION}/{}",
            self.asset_name
        )
    }
}

fn package_for_platform(os: &str, arch: &str) -> Option<OnnxRuntimePackage> {
    let (platform, archive_kind) = match (os, arch) {
        ("linux", "x86_64") => ("linux-x64", ArchiveKind::Tgz),
        ("linux", "aarch64") => ("linux-aarch64", ArchiveKind::Tgz),
        ("macos", "x86_64") => ("osx-x86_64", ArchiveKind::Tgz),
        ("macos", "aarch64") => ("osx-arm64", ArchiveKind::Tgz),
        ("windows", "x86_64") => ("win-x64", ArchiveKind::Zip),
        ("windows", "aarch64") => ("win-arm64", ArchiveKind::Zip),
        _ => return None,
    };
    let suffix = match archive_kind {
        ArchiveKind::Tgz => "tgz",
        ArchiveKind::Zip => "zip",
    };
    let asset_name = match (platform, suffix) {
        ("linux-x64", "tgz") => "onnxruntime-linux-x64-1.23.2.tgz",
        ("linux-aarch64", "tgz") => "onnxruntime-linux-aarch64-1.23.2.tgz",
        ("osx-x86_64", "tgz") => "onnxruntime-osx-x86_64-1.23.2.tgz",
        ("osx-arm64", "tgz") => "onnxruntime-osx-arm64-1.23.2.tgz",
        ("win-x64", "zip") => "onnxruntime-win-x64-1.23.2.zip",
        ("win-arm64", "zip") => "onnxruntime-win-arm64-1.23.2.zip",
        _ => return None,
    };
    Some(OnnxRuntimePackage {
        asset_name,
        archive_kind,
    })
}

async fn install_onnxruntime(package: OnnxRuntimePackage, target_dir: &Path) -> Result<()> {
    fs::create_dir_all(target_dir)
        .with_context(|| format!("创建 ONNX Runtime 安装目录失败：{}", target_dir.display()))?;

    let temp_dir = Builder::new()
        .prefix("codex-zinit-onnxruntime-")
        .tempdir()
        .context("创建临时目录失败")?;
    let temp_root = temp_dir.path().to_path_buf();
    let archive_path = temp_root.join(package.asset_name);
    download_archive(&package.url(), &archive_path).await?;
    extract_archive(package.archive_kind, &archive_path, &temp_root).await?;

    let source =
        find_file_named(&temp_root, default_onnxruntime_dylib_name())?.with_context(|| {
            format!(
                "ONNX Runtime 安装包中未找到 {}",
                default_onnxruntime_dylib_name()
            )
        })?;
    let destination = target_dir.join(default_onnxruntime_dylib_name());
    fs::copy(&source, &destination)
        .with_context(|| format!("复制 ONNX Runtime 动态库到 {} 失败", destination.display()))?;
    Ok(())
}

async fn download_archive(url: &str, dest: &Path) -> Result<()> {
    let status = Command::new("curl")
        .arg("-fL")
        .arg("--retry")
        .arg("3")
        .arg("--retry-delay")
        .arg("1")
        .arg("-o")
        .arg(dest)
        .arg(url)
        .status()
        .await
        .context("调用 `curl` 下载 ONNX Runtime 失败")?;

    if status.success() {
        return Ok(());
    }
    bail!("curl 下载 ONNX Runtime 失败，状态码：{status}");
}

async fn extract_archive(kind: ArchiveKind, archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let mut command = match kind {
        ArchiveKind::Tgz => {
            let mut command = Command::new("tar");
            command
                .arg("-xzf")
                .arg(archive_path)
                .arg("-C")
                .arg(dest_dir);
            command
        }
        ArchiveKind::Zip => {
            let mut command = Command::new("powershell");
            command
                .arg("-NoProfile")
                .arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-Command")
                .arg("Expand-Archive -Force -LiteralPath $args[0] -DestinationPath $args[1]")
                .arg(archive_path)
                .arg(dest_dir);
            command
        }
    };
    let status = command.status().await.context("解压 ONNX Runtime 失败")?;

    if status.success() {
        return Ok(());
    }
    bail!("解压 ONNX Runtime 失败，状态码：{status}");
}

fn find_file_named(root: &Path, file_name: &str) -> Result<Option<PathBuf>> {
    for entry in fs::read_dir(root).with_context(|| format!("读取目录失败：{}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
            return Ok(Some(path));
        }
        if path.is_dir()
            && let Some(found) = find_file_named(&path, file_name)?
        {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

#[derive(Debug)]
struct OnnxRuntimeCheck {
    ready: bool,
    path: Option<PathBuf>,
    reason: Option<String>,
}

fn check_onnxruntime(default_path: &Path) -> OnnxRuntimeCheck {
    let path = resolved_onnxruntime_dylib_path(default_path);
    if !path.exists() {
        return OnnxRuntimeCheck {
            ready: false,
            path: Some(path.clone()),
            reason: Some(format!("未找到 {}", path.display())),
        };
    }

    match ensure_onnxruntime_dylib_loadable_at(&path) {
        Ok(()) => OnnxRuntimeCheck {
            ready: true,
            path: Some(path),
            reason: None,
        },
        Err(err) => OnnxRuntimeCheck {
            ready: false,
            path: Some(path),
            reason: Some(err.to_string()),
        },
    }
}

fn resolved_onnxruntime_dylib_path(default_path: &Path) -> PathBuf {
    std::env::var_os("ORT_DYLIB_PATH")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_path.to_path_buf())
}

fn default_install_dir() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("读取当前 codex 可执行文件路径失败")?;
    current_exe
        .parent()
        .map(Path::to_path_buf)
        .context("当前 codex 可执行文件没有父目录")
}

fn default_onnxruntime_dylib_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "onnxruntime.dll"
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        "libonnxruntime.so"
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        "libonnxruntime.dylib"
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    )))]
    {
        "libonnxruntime.so"
    }
}

#[cfg(unix)]
fn ensure_onnxruntime_dylib_loadable_at(path: &Path) -> Result<()> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .with_context(|| format!("无效的 ONNX Runtime 动态库路径：{}", path.display()))?;
    let symbol = c"OrtGetApiBase";

    unsafe {
        let _ = libc::dlerror();
    }

    let handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        let detail = dlerror_message().unwrap_or_else(|| "unknown dlopen error".to_string());
        bail!("{} 不可加载：{detail}", path.display());
    }

    unsafe {
        let _ = libc::dlerror();
    }
    let symbol_ptr = unsafe { libc::dlsym(handle, symbol.as_ptr()) };
    let symbol_error = dlerror_message();
    unsafe {
        libc::dlclose(handle);
    }

    if symbol_ptr.is_null() {
        let detail = symbol_error.unwrap_or_else(|| "缺少 `OrtGetApiBase` 符号".to_string());
        bail!("{} 不可用：{detail}", path.display());
    }

    Ok(())
}

#[cfg(unix)]
fn dlerror_message() -> Option<String> {
    let error = unsafe { libc::dlerror() };
    if error.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(not(unix))]
fn ensure_onnxruntime_dylib_loadable_at(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn package_mapping_matches_supported_platforms() {
        assert_eq!(
            package_for_platform("linux", "x86_64"),
            Some(OnnxRuntimePackage {
                asset_name: "onnxruntime-linux-x64-1.23.2.tgz",
                archive_kind: ArchiveKind::Tgz,
            })
        );
        assert_eq!(
            package_for_platform("macos", "aarch64"),
            Some(OnnxRuntimePackage {
                asset_name: "onnxruntime-osx-arm64-1.23.2.tgz",
                archive_kind: ArchiveKind::Tgz,
            })
        );
        assert_eq!(
            package_for_platform("windows", "x86_64"),
            Some(OnnxRuntimePackage {
                asset_name: "onnxruntime-win-x64-1.23.2.zip",
                archive_kind: ArchiveKind::Zip,
            })
        );
        assert_eq!(package_for_platform("freebsd", "x86_64"), None);
    }

    #[test]
    fn package_url_uses_pinned_onnxruntime_release() {
        let package = package_for_platform("linux", "x86_64").expect("supported platform");
        assert_eq!(
            package.url(),
            "https://github.com/microsoft/onnxruntime/releases/download/v1.23.2/onnxruntime-linux-x64-1.23.2.tgz"
        );
    }

    #[test]
    fn install_target_prefers_ort_dylib_path() {
        let target = resolve_install_target_with(
            Some(PathBuf::from("/ignored")),
            Some(std::ffi::OsString::from("/custom/libonnxruntime.so")),
            || Ok(PathBuf::from("/default")),
        )
        .expect("target should resolve");

        assert_eq!(
            target.dylib_path,
            PathBuf::from("/custom/libonnxruntime.so")
        );
    }

    #[test]
    fn install_target_uses_target_dir_without_ort_dylib_path() {
        let target = resolve_install_target_with(Some(PathBuf::from("/custom-dir")), None, || {
            Ok(PathBuf::from("/default"))
        })
        .expect("target should resolve");

        assert_eq!(
            target.dylib_path,
            PathBuf::from("/custom-dir").join(default_onnxruntime_dylib_name())
        );
    }
}
