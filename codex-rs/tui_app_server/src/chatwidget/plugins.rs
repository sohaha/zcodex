use std::path::PathBuf;

use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::bottom_pane::ColumnWidthMode;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::history_cell;
use crate::render::renderable::ColumnRenderable;
use codex_app_server_protocol::PluginDetail;
use codex_app_server_protocol::PluginInstallPolicy;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginMarketplaceEntry;
use codex_app_server_protocol::PluginReadResponse;
use codex_app_server_protocol::PluginSummary;
use codex_core::plugins::OPENAI_CURATED_MARKETPLACE_NAME;
use codex_features::Feature;
use ratatui::style::Stylize;
use ratatui::text::Line;

const PLUGINS_SELECTION_VIEW_ID: &str = "plugins-selection";
const SUPPORTED_MARKETPLACE_NAME: &str = OPENAI_CURATED_MARKETPLACE_NAME;

#[derive(Debug, Clone, Default)]
pub(super) enum PluginsCacheState {
    #[default]
    Uninitialized,
    Loading,
    Ready(PluginListResponse),
    Failed(String),
}

impl ChatWidget {
    pub(crate) fn add_plugins_output(&mut self) {
        if !self.config.features.enabled(Feature::Plugins) {
            self.add_info_message(
                "Plugins are disabled.".to_string(),
                Some("Enable the plugins feature to use /plugins.".to_string()),
            );
            return;
        }

        self.prefetch_plugins();

        match self.plugins_cache_for_current_cwd() {
            PluginsCacheState::Ready(response) => {
                self.open_plugins_popup(&response);
            }
            PluginsCacheState::Failed(err) => {
                self.add_to_history(history_cell::new_error_event(err));
            }
            PluginsCacheState::Loading | PluginsCacheState::Uninitialized => {
                self.open_plugins_loading_popup();
            }
        }
        self.request_redraw();
    }

    pub(crate) fn on_plugins_loaded(
        &mut self,
        cwd: PathBuf,
        result: Result<PluginListResponse, String>,
    ) {
        if self.plugins_fetch_state.in_flight_cwd.as_ref() == Some(&cwd) {
            self.plugins_fetch_state.in_flight_cwd = None;
        }

        if self.config.cwd != cwd {
            return;
        }

        match result {
            Ok(response) => {
                self.plugins_fetch_state.cache_cwd = Some(cwd);
                self.plugins_cache = PluginsCacheState::Ready(response.clone());
                self.refresh_plugins_popup_if_open(&response);
            }
            Err(err) => {
                self.plugins_fetch_state.cache_cwd = None;
                self.plugins_cache = PluginsCacheState::Failed(err.clone());
                let _ = self.bottom_pane.replace_selection_view_if_active(
                    PLUGINS_SELECTION_VIEW_ID,
                    self.plugins_error_popup_params(&err),
                );
            }
        }
    }

    fn prefetch_plugins(&mut self) {
        let cwd = self.config.cwd.clone();
        if self.plugins_fetch_state.in_flight_cwd.as_ref() == Some(&cwd) {
            return;
        }

        self.plugins_fetch_state.in_flight_cwd = Some(cwd.clone());
        if self.plugins_fetch_state.cache_cwd.as_ref() != Some(&cwd) {
            self.plugins_cache = PluginsCacheState::Loading;
        }

        self.app_event_tx.send(AppEvent::FetchPluginsList { cwd });
    }

    fn plugins_cache_for_current_cwd(&self) -> PluginsCacheState {
        if self.plugins_fetch_state.cache_cwd.as_ref() == Some(&self.config.cwd) {
            self.plugins_cache.clone()
        } else {
            PluginsCacheState::Uninitialized
        }
    }

    fn open_plugins_loading_popup(&mut self) {
        if !self.bottom_pane.replace_selection_view_if_active(
            PLUGINS_SELECTION_VIEW_ID,
            self.plugins_loading_popup_params(),
        ) {
            self.bottom_pane
                .show_selection_view(self.plugins_loading_popup_params());
        }
    }

    fn open_plugins_popup(&mut self, response: &PluginListResponse) {
        self.bottom_pane
            .show_selection_view(self.plugins_popup_params(response));
    }

    pub(crate) fn open_plugin_detail_loading_popup(&mut self, plugin_display_name: &str) {
        let params = self.plugin_detail_loading_popup_params(plugin_display_name);
        let _ = self
            .bottom_pane
            .replace_selection_view_if_active(PLUGINS_SELECTION_VIEW_ID, params);
    }

    pub(crate) fn on_plugin_detail_loaded(
        &mut self,
        cwd: PathBuf,
        result: Result<PluginReadResponse, String>,
    ) {
        if self.config.cwd != cwd {
            return;
        }

        let plugins_response = match self.plugins_cache_for_current_cwd() {
            PluginsCacheState::Ready(response) => Some(response),
            _ => None,
        };

        match result {
            Ok(response) => {
                if let Some(plugins_response) = plugins_response {
                    let _ = self.bottom_pane.replace_selection_view_if_active(
                        PLUGINS_SELECTION_VIEW_ID,
                        self.plugin_detail_popup_params(&plugins_response, &response.plugin),
                    );
                }
            }
            Err(err) => {
                let _ = self.bottom_pane.replace_selection_view_if_active(
                    PLUGINS_SELECTION_VIEW_ID,
                    self.plugin_detail_error_popup_params(&err, plugins_response.as_ref()),
                );
            }
        }
    }

    fn refresh_plugins_popup_if_open(&mut self, response: &PluginListResponse) {
        let _ = self.bottom_pane.replace_selection_view_if_active(
            PLUGINS_SELECTION_VIEW_ID,
            self.plugins_popup_params(response),
        );
    }

    fn plugins_loading_popup_params(&self) -> SelectionViewParams {
        let mut header = ColumnRenderable::new();
        header.push(Line::from("插件".bold()));
        header.push(Line::from("正在加载可用插件...".dim()));
        header.push(Line::from("当前仅展示 ChatGPT 市场的插件。".dim()));

        SelectionViewParams {
            view_id: Some(PLUGINS_SELECTION_VIEW_ID),
            header: Box::new(header),
            items: vec![SelectionItem {
                name: "正在加载插件...".to_string(),
                description: Some("市场列表准备好后会更新此处。".to_string()),
                is_disabled: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn plugin_detail_loading_popup_params(&self, plugin_display_name: &str) -> SelectionViewParams {
        let mut header = ColumnRenderable::new();
        header.push(Line::from("插件".bold()));
        header.push(Line::from(
            format!("正在加载 {plugin_display_name} 的详情...").dim(),
        ));

        SelectionViewParams {
            view_id: Some(PLUGINS_SELECTION_VIEW_ID),
            header: Box::new(header),
            items: vec![SelectionItem {
                name: "正在加载插件详情...".to_string(),
                description: Some("插件详情请求完成后会更新此处。".to_string()),
                is_disabled: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn plugins_error_popup_params(&self, err: &str) -> SelectionViewParams {
        let mut header = ColumnRenderable::new();
        header.push(Line::from("插件".bold()));
        header.push(Line::from("加载插件失败。".dim()));

        SelectionViewParams {
            view_id: Some(PLUGINS_SELECTION_VIEW_ID),
            header: Box::new(header),
            items: vec![SelectionItem {
                name: "插件市场暂不可用".to_string(),
                description: Some(err.to_string()),
                is_disabled: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn plugin_detail_error_popup_params(
        &self,
        err: &str,
        plugins_response: Option<&PluginListResponse>,
    ) -> SelectionViewParams {
        let mut header = ColumnRenderable::new();
        header.push(Line::from("插件".bold()));
        header.push(Line::from("加载插件详情失败。".dim()));

        let mut items = vec![SelectionItem {
            name: "插件详情暂不可用".to_string(),
            description: Some(err.to_string()),
            is_disabled: true,
            ..Default::default()
        }];
        if let Some(plugins_response) = plugins_response.cloned() {
            let cwd = self.config.cwd.clone();
            items.push(SelectionItem {
                name: "返回插件列表".to_string(),
                description: Some("回到插件列表。".to_string()),
                selected_description: Some("回到插件列表。".to_string()),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::PluginsLoaded {
                        cwd: cwd.clone(),
                        result: Ok(plugins_response.clone()),
                    });
                })],
                ..Default::default()
            });
        }

        SelectionViewParams {
            view_id: Some(PLUGINS_SELECTION_VIEW_ID),
            header: Box::new(header),
            footer_hint: Some(plugins_popup_hint_line()),
            items,
            ..Default::default()
        }
    }

    fn plugins_popup_params(&self, response: &PluginListResponse) -> SelectionViewParams {
        let marketplaces: Vec<&PluginMarketplaceEntry> = response
            .marketplaces
            .iter()
            .filter(|marketplace| marketplace.name == SUPPORTED_MARKETPLACE_NAME)
            .collect();

        let total: usize = marketplaces
            .iter()
            .map(|marketplace| marketplace.plugins.len())
            .sum();
        let installed = marketplaces
            .iter()
            .flat_map(|marketplace| marketplace.plugins.iter())
            .filter(|plugin| plugin.installed)
            .count();

        let mut header = ColumnRenderable::new();
        header.push(Line::from("插件".bold()));
        header.push(Line::from("浏览来自 ChatGPT 市场的插件。".dim()));
        header.push(Line::from(
            format!("已安装 {installed} / 共 {total} 个可用插件。").dim(),
        ));
        if let Some(remote_sync_error) = response.remote_sync_error.as_deref() {
            header.push(Line::from(
                format!("Using cached marketplace data: {remote_sync_error}").dim(),
            ));
        }

        let mut items: Vec<SelectionItem> = Vec::new();
        for marketplace in marketplaces {
            let marketplace_label = marketplace_display_name(marketplace);
            for plugin in &marketplace.plugins {
                let display_name = plugin_display_name(plugin);
                let status_label = plugin_status_label(plugin);
                let description = plugin_brief_description(plugin, &marketplace_label);
                let selected_description = format!("{status_label}。按 Enter 查看插件详情。");
                let search_value = format!(
                    "{display_name} {} {} {}",
                    plugin.id, plugin.name, marketplace_label
                );
                let cwd = self.config.cwd.clone();
                let plugin_display_name = display_name.clone();
                let marketplace_path = marketplace.path.clone();
                let plugin_name = plugin.name.clone();

                items.push(SelectionItem {
                    name: format!("{display_name} · {marketplace_label}"),
                    description: Some(description),
                    selected_description: Some(selected_description),
                    search_value: Some(search_value),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::OpenPluginDetailLoading {
                            plugin_display_name: plugin_display_name.clone(),
                        });
                        tx.send(AppEvent::FetchPluginDetail {
                            cwd: cwd.clone(),
                            params: codex_app_server_protocol::PluginReadParams {
                                marketplace_path: marketplace_path.clone(),
                                plugin_name: plugin_name.clone(),
                            },
                        });
                    })],
                    ..Default::default()
                });
            }
        }

        if items.is_empty() {
            items.push(SelectionItem {
                name: "ChatGPT 市场暂无可用插件".to_string(),
                description: Some("当前仅展示 ChatGPT 插件市场。".to_string()),
                is_disabled: true,
                ..Default::default()
            });
        }

        SelectionViewParams {
            view_id: Some(PLUGINS_SELECTION_VIEW_ID),
            header: Box::new(header),
            footer_hint: Some(plugins_popup_hint_line()),
            items,
            is_searchable: true,
            search_placeholder: Some("输入以搜索插件".to_string()),
            col_width_mode: ColumnWidthMode::AutoAllRows,
            ..Default::default()
        }
    }

    fn plugin_detail_popup_params(
        &self,
        plugins_response: &PluginListResponse,
        plugin: &PluginDetail,
    ) -> SelectionViewParams {
        let marketplace_label = plugin.marketplace_name.clone();
        let display_name = plugin_display_name(&plugin.summary);
        let status_label = plugin_status_label(&plugin.summary);
        let mut header = ColumnRenderable::new();
        header.push(Line::from("插件".bold()));
        header.push(Line::from(
            format!("{display_name} · {marketplace_label}").bold(),
        ));
        header.push(Line::from(status_label.dim()));
        if let Some(description) = plugin_detail_description(plugin) {
            header.push(Line::from(description.dim()));
        }

        let cwd = self.config.cwd.clone();
        let plugins_response = plugins_response.clone();
        let mut items = vec![SelectionItem {
            name: "返回插件列表".to_string(),
            description: Some("回到插件列表。".to_string()),
            selected_description: Some("回到插件列表。".to_string()),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::PluginsLoaded {
                    cwd: cwd.clone(),
                    result: Ok(plugins_response.clone()),
                });
            })],
            ..Default::default()
        }];

        items.push(SelectionItem {
            name: "Skills".to_string(),
            description: Some(plugin_skill_summary(plugin)),
            is_disabled: true,
            ..Default::default()
        });
        items.push(SelectionItem {
            name: "Apps".to_string(),
            description: Some(plugin_app_summary(plugin)),
            is_disabled: true,
            ..Default::default()
        });
        items.push(SelectionItem {
            name: "MCP Servers".to_string(),
            description: Some(plugin_mcp_summary(plugin)),
            is_disabled: true,
            ..Default::default()
        });

        SelectionViewParams {
            view_id: Some(PLUGINS_SELECTION_VIEW_ID),
            header: Box::new(header),
            footer_hint: Some(plugins_popup_hint_line()),
            items,
            col_width_mode: ColumnWidthMode::AutoAllRows,
            ..Default::default()
        }
    }
}

fn plugins_popup_hint_line() -> Line<'static> {
    Line::from("按 esc 关闭。")
}

fn marketplace_display_name(marketplace: &PluginMarketplaceEntry) -> String {
    marketplace
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .map(str::trim)
        .filter(|display_name| !display_name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| marketplace.name.clone())
}

fn plugin_display_name(plugin: &PluginSummary) -> String {
    plugin
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .map(str::trim)
        .filter(|display_name| !display_name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| plugin.name.clone())
}

fn plugin_brief_description(plugin: &PluginSummary, marketplace_label: &str) -> String {
    let status_label = plugin_status_label(plugin);
    match plugin_description(plugin) {
        Some(description) => format!("{status_label} · {marketplace_label} · {description}"),
        None => format!("{status_label} · {marketplace_label}"),
    }
}

fn plugin_status_label(plugin: &PluginSummary) -> &'static str {
    if plugin.installed {
        if plugin.enabled {
            "已安装"
        } else {
            "已安装 · 已禁用"
        }
    } else {
        match plugin.install_policy {
            PluginInstallPolicy::NotAvailable => "不可安装",
            PluginInstallPolicy::Available => "可安装",
            PluginInstallPolicy::InstalledByDefault => "默认可用",
        }
    }
}

fn plugin_description(plugin: &PluginSummary) -> Option<String> {
    plugin
        .interface
        .as_ref()
        .and_then(|interface| {
            interface
                .short_description
                .as_deref()
                .or(interface.long_description.as_deref())
        })
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .map(str::to_string)
}

fn plugin_detail_description(plugin: &PluginDetail) -> Option<String> {
    plugin
        .description
        .as_deref()
        .or_else(|| {
            plugin
                .summary
                .interface
                .as_ref()
                .and_then(|interface| interface.long_description.as_deref())
        })
        .or_else(|| {
            plugin
                .summary
                .interface
                .as_ref()
                .and_then(|interface| interface.short_description.as_deref())
        })
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .map(str::to_string)
}

fn plugin_skill_summary(plugin: &PluginDetail) -> String {
    if plugin.skills.is_empty() {
        "暂无插件 Skills。".to_string()
    } else {
        plugin
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn plugin_app_summary(plugin: &PluginDetail) -> String {
    if plugin.apps.is_empty() {
        "暂无插件 Apps。".to_string()
    } else {
        plugin
            .apps
            .iter()
            .map(|app| app.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn plugin_mcp_summary(plugin: &PluginDetail) -> String {
    if plugin.mcp_servers.is_empty() {
        "暂无插件 MCP 服务器。".to_string()
    } else {
        plugin.mcp_servers.join(", ")
    }
}
