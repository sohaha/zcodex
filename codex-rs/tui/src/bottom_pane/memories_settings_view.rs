use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::key_hint;
use crate::render::Insets;
use crate::render::RectExt as _;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;
use crate::style::user_message_style;

use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::popup_consts::MAX_POPUP_ROWS;
use super::popup_consts::standard_popup_hint_line;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::measure_rows_height;
use super::selection_popup_common::render_rows;

const MEMORIES_DOC_URL: &str = "https://developers.openai.com/codex/memories";

#[derive(Clone, Copy, PartialEq, Eq)]
enum MemoriesSetting {
    Use,
    Generate,
    Reset,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MemoriesViewMode {
    Settings,
    ResetConfirmation,
}

struct MemoriesSettingItem {
    setting: MemoriesSetting,
    name: &'static str,
    description: &'static str,
    enabled: bool,
}

pub(crate) struct MemoriesSettingsView {
    items: Vec<MemoriesSettingItem>,
    state: ScrollState,
    complete: bool,
    mode: MemoriesViewMode,
    app_event_tx: AppEventSender,
    docs_link: Line<'static>,
}

impl MemoriesSettingsView {
    pub(crate) fn new(
        use_memories: bool,
        generate_memories: bool,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut view = Self {
            items: vec![
                MemoriesSettingItem {
                    setting: MemoriesSetting::Use,
                    name: "使用记忆",
                    description: "在以下线程中使用记忆。将在下一个线程生效。",
                    enabled: use_memories,
                },
                MemoriesSettingItem {
                    setting: MemoriesSetting::Generate,
                    name: "生成记忆",
                    description: "从以下线程生成记忆，当前线程也会被包含。",
                    enabled: generate_memories,
                },
                MemoriesSettingItem {
                    setting: MemoriesSetting::Reset,
                    name: "重置所有记忆",
                    description: "清除本地记忆文件和摘要。现有线程将保持不变。",
                    enabled: false,
                },
            ],
            state: ScrollState::new(),
            complete: false,
            mode: MemoriesViewMode::Settings,
            app_event_tx,
            docs_link: Line::from(vec![
                "了解更多：".dim(),
                MEMORIES_DOC_URL.cyan().underlined(),
            ]),
        };
        view.initialize_selection();
        view
    }

    fn initialize_selection(&mut self) {
        self.state.selected_idx = (!self.items.is_empty()).then_some(0);
    }

    fn visible_len(&self) -> usize {
        match self.mode {
            MemoriesViewMode::Settings => self.items.len(),
            MemoriesViewMode::ResetConfirmation => 2,
        }
    }

    fn build_rows(&self) -> Vec<GenericDisplayRow> {
        if self.mode == MemoriesViewMode::ResetConfirmation {
            return self.build_reset_confirmation_rows();
        }

        let mut rows = Vec::with_capacity(self.items.len());
        let selected_idx = self.state.selected_idx;
        for (idx, item) in self.items.iter().enumerate() {
            let prefix = if selected_idx == Some(idx) {
                '›'
            } else {
                ' '
            };
            let name = match item.setting {
                MemoriesSetting::Reset => format!("{prefix} {}", item.name),
                MemoriesSetting::Use | MemoriesSetting::Generate => {
                    let marker = if item.enabled { 'x' } else { ' ' };
                    format!("{prefix} [{marker}] {}", item.name)
                }
            };
            rows.push(GenericDisplayRow {
                name,
                description: Some(item.description.to_string()),
                ..Default::default()
            });
        }

        rows
    }

    fn build_reset_confirmation_rows(&self) -> Vec<GenericDisplayRow> {
        let selected_idx = self.state.selected_idx;
        vec![
            GenericDisplayRow {
                name: if selected_idx == Some(0) {
                    "› 重置所有记忆".to_string()
                } else {
                    "  重置所有记忆".to_string()
                },
                description: Some("删除本地记忆文件和 rollout 摘要。".to_string()),
                ..Default::default()
            },
            GenericDisplayRow {
                name: if selected_idx == Some(1) {
                    "› 返回".to_string()
                } else {
                    "  返回".to_string()
                },
                description: Some("返回记忆设置。".to_string()),
                ..Default::default()
            },
        ]
    }

    fn move_up(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            return;
        }
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    fn move_down(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            return;
        }
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    fn toggle_selected(&mut self) {
        if self.mode != MemoriesViewMode::Settings {
            return;
        }

        let Some(selected_idx) = self.state.selected_idx else {
            return;
        };

        if let Some(item) = self.items.get_mut(selected_idx)
            && item.setting != MemoriesSetting::Reset
        {
            item.enabled = !item.enabled;
        }
    }

    fn rows_width(total_width: u16) -> u16 {
        total_width.saturating_sub(2)
    }

    fn current_setting(&self, setting: MemoriesSetting) -> bool {
        self.items
            .iter()
            .find(|item| item.setting == setting)
            .is_some_and(|item| item.enabled)
    }

    fn build_header(&self) -> Box<dyn Renderable> {
        let mut header = ColumnRenderable::new();
        match self.mode {
            MemoriesViewMode::Settings => {
                header.push(Line::from("记忆".bold()));
                header.push(Line::from(
                    "选择 Codex 如何使用和生成记忆。更改将保存到 config.toml。".dim(),
                ));
            }
            MemoriesViewMode::ResetConfirmation => {
                header.push(Line::from("重置所有记忆？".bold()));
            }
        }
        Box::new(header)
    }

    fn footer_hint(&self) -> Line<'static> {
        match self.mode {
            MemoriesViewMode::Settings => memories_settings_hint_line(),
            MemoriesViewMode::ResetConfirmation => standard_popup_hint_line(),
        }
    }
}

impl BottomPaneView for MemoriesSettingsView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{0010}'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('\u{000e}'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.toggle_selected(),
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.save(),
            KeyEvent {
                code: KeyCode::Esc, ..
            } => self.cancel(),
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.cancel();
        CancellationEvent::Handled
    }
}

impl MemoriesSettingsView {
    fn save(&mut self) {
        match self.mode {
            MemoriesViewMode::Settings => {
                if self
                    .items
                    .get(self.state.selected_idx.unwrap_or_default())
                    .is_some_and(|item| item.setting == MemoriesSetting::Reset)
                {
                    self.mode = MemoriesViewMode::ResetConfirmation;
                    self.state.selected_idx = Some(0);
                    return;
                }

                self.app_event_tx.send(AppEvent::UpdateMemorySettings {
                    use_memories: self.current_setting(MemoriesSetting::Use),
                    generate_memories: self.current_setting(MemoriesSetting::Generate),
                });
                self.complete = true;
            }
            MemoriesViewMode::ResetConfirmation => match self.state.selected_idx {
                Some(0) => {
                    self.app_event_tx.send(AppEvent::ResetMemories);
                    self.complete = true;
                }
                Some(1) => {
                    self.mode = MemoriesViewMode::Settings;
                    self.state.selected_idx = self
                        .items
                        .iter()
                        .position(|item| item.setting == MemoriesSetting::Reset);
                }
                _ => {}
            },
        }
    }

    fn cancel(&mut self) {
        if self.mode == MemoriesViewMode::ResetConfirmation {
            self.mode = MemoriesViewMode::Settings;
            self.state.selected_idx = self
                .items
                .iter()
                .position(|item| item.setting == MemoriesSetting::Reset);
            return;
        }

        self.complete = true;
    }
}

impl Renderable for MemoriesSettingsView {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let [content_area, footer_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        Block::default()
            .style(user_message_style())
            .render(content_area, buf);

        let header = self.build_header();
        let header_height = header.desired_height(content_area.width.saturating_sub(4));
        let rows = self.build_rows();
        let rows_width = Self::rows_width(content_area.width);
        let rows_height = measure_rows_height(
            &rows,
            &self.state,
            MAX_POPUP_ROWS,
            rows_width.saturating_add(1),
        );
        let layout_area = content_area.inset(Insets::vh(/*v*/ 1, /*h*/ 2));
        let [header_area, _, list_area, _, footer_note_area] =
            if self.mode == MemoriesViewMode::Settings {
                Layout::vertical([
                    Constraint::Max(header_height),
                    Constraint::Max(1),
                    Constraint::Length(rows_height),
                    Constraint::Max(1),
                    Constraint::Length(1),
                ])
                .areas(layout_area)
            } else {
                Layout::vertical([
                    Constraint::Max(header_height),
                    Constraint::Max(1),
                    Constraint::Length(rows_height),
                    Constraint::Max(0),
                    Constraint::Length(0),
                ])
                .areas(layout_area)
            };

        header.render(header_area, buf);

        if list_area.height > 0 {
            let render_area = Rect {
                x: list_area.x.saturating_sub(2),
                y: list_area.y,
                width: rows_width.max(1),
                height: list_area.height,
            };
            render_rows(
                render_area,
                buf,
                &rows,
                &self.state,
                MAX_POPUP_ROWS,
                "  No memory settings available",
            );
        }
        if self.mode == MemoriesViewMode::Settings {
            self.docs_link.clone().render(footer_note_area, buf);
        }

        let hint_area = Rect {
            x: footer_area.x + 2,
            y: footer_area.y,
            width: footer_area.width.saturating_sub(2),
            height: footer_area.height,
        };
        self.footer_hint().dim().render(hint_area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let header = self.build_header();
        let rows = self.build_rows();
        let rows_width = Self::rows_width(width);
        let rows_height = measure_rows_height(
            &rows,
            &self.state,
            MAX_POPUP_ROWS,
            rows_width.saturating_add(1),
        );

        let mut height = header.desired_height(width.saturating_sub(4));
        height = height.saturating_add(rows_height + 4);
        if self.mode == MemoriesViewMode::Settings {
            height = height.saturating_add(1);
        }
        height.saturating_add(1)
    }
}

fn memories_settings_hint_line() -> Line<'static> {
    Line::from(vec![
        "按 ".into(),
        key_hint::plain(KeyCode::Char(' ')).into(),
        " 切换；按 ".into(),
        key_hint::plain(KeyCode::Enter).into(),
        " 保存或选择".into(),
    ])
}
