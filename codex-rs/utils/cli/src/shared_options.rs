//! Shared command-line flags used by both interactive and non-interactive Codex entry points.

use crate::SandboxModeCliArg;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug, Default)]
pub struct SharedCliOptions {
    /// 可选的初始提示附加图片。
    #[arg(
        long = "image",
        short = 'i',
        value_name = "FILE",
        value_delimiter = ',',
        num_args = 1..
    )]
    pub images: Vec<PathBuf>,

    /// Agent 应使用的模型。
    #[arg(long, short = 'm')]
    pub model: Option<String>,

    /// 使用开源 provider。
    #[arg(long = "oss", default_value_t = false)]
    pub oss: bool,

    /// 指定要使用的本地 provider（lmstudio 或 ollama）。
    /// 若未与 `--oss` 一起指定，则使用配置默认值或显示选择界面。
    #[arg(long = "local-provider")]
    pub oss_provider: Option<String>,

    /// 从 config.toml 中选择配置 profile 以指定默认选项。
    #[arg(long = "profile", short = 'p')]
    pub config_profile: Option<String>,

    /// 选择执行模型生成 shell 命令时使用的沙盒策略。
    #[arg(long = "sandbox", short = 's')]
    pub sandbox_mode: Option<SandboxModeCliArg>,

    /// 低摩擦沙盒自动执行的便捷别名。
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    /// 跳过所有确认提示，并在无沙盒下执行命令。
    /// 极度危险。仅适用于运行在外部已隔离环境中的场景。
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        conflicts_with = "full_auto"
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// 让 agent 使用指定目录作为工作根目录。
    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// 指定除主工作区外还应允许写入的额外目录。
    #[arg(long = "add-dir", value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    pub add_dir: Vec<PathBuf>,
}

impl SharedCliOptions {
    pub fn inherit_exec_root_options(&mut self, root: &Self) {
        let self_selected_sandbox_mode = self.sandbox_mode.is_some()
            || self.full_auto
            || self.dangerously_bypass_approvals_and_sandbox;
        let Self {
            images,
            model,
            oss,
            oss_provider,
            config_profile,
            sandbox_mode,
            full_auto,
            dangerously_bypass_approvals_and_sandbox,
            cwd,
            add_dir,
        } = self;
        let Self {
            images: root_images,
            model: root_model,
            oss: root_oss,
            oss_provider: root_oss_provider,
            config_profile: root_config_profile,
            sandbox_mode: root_sandbox_mode,
            full_auto: root_full_auto,
            dangerously_bypass_approvals_and_sandbox: root_dangerously_bypass_approvals_and_sandbox,
            cwd: root_cwd,
            add_dir: root_add_dir,
        } = root;

        if model.is_none() {
            model.clone_from(root_model);
        }
        if *root_oss {
            *oss = true;
        }
        if oss_provider.is_none() {
            oss_provider.clone_from(root_oss_provider);
        }
        if config_profile.is_none() {
            config_profile.clone_from(root_config_profile);
        }
        if sandbox_mode.is_none() {
            *sandbox_mode = *root_sandbox_mode;
        }
        if !self_selected_sandbox_mode {
            *full_auto = *root_full_auto;
            *dangerously_bypass_approvals_and_sandbox =
                *root_dangerously_bypass_approvals_and_sandbox;
        }
        if cwd.is_none() {
            cwd.clone_from(root_cwd);
        }
        if !root_images.is_empty() {
            let mut merged_images = root_images.clone();
            merged_images.append(images);
            *images = merged_images;
        }
        if !root_add_dir.is_empty() {
            let mut merged_add_dir = root_add_dir.clone();
            merged_add_dir.append(add_dir);
            *add_dir = merged_add_dir;
        }
    }

    pub fn apply_subcommand_overrides(&mut self, subcommand: Self) {
        let subcommand_selected_sandbox_mode = subcommand.sandbox_mode.is_some()
            || subcommand.full_auto
            || subcommand.dangerously_bypass_approvals_and_sandbox;
        let Self {
            images,
            model,
            oss,
            oss_provider,
            config_profile,
            sandbox_mode,
            full_auto,
            dangerously_bypass_approvals_and_sandbox,
            cwd,
            add_dir,
        } = subcommand;

        if let Some(model) = model {
            self.model = Some(model);
        }
        if oss {
            self.oss = true;
        }
        if let Some(oss_provider) = oss_provider {
            self.oss_provider = Some(oss_provider);
        }
        if let Some(config_profile) = config_profile {
            self.config_profile = Some(config_profile);
        }
        if subcommand_selected_sandbox_mode {
            self.sandbox_mode = sandbox_mode;
            self.full_auto = full_auto;
            self.dangerously_bypass_approvals_and_sandbox =
                dangerously_bypass_approvals_and_sandbox;
        }
        if let Some(cwd) = cwd {
            self.cwd = Some(cwd);
        }
        if !images.is_empty() {
            self.images = images;
        }
        if !add_dir.is_empty() {
            self.add_dir.extend(add_dir);
        }
    }
}
