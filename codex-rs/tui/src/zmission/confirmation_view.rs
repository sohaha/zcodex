//! Phase 确认视图。
//!
//! 当子代理完成阶段分析后，弹出确认面板让用户选择：
//! - 确认继续：推进到下一阶段
//! - 补充内容：继续与当前子代理沟通

use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::ViewCompletion;
use crate::render::renderable::Renderable;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;

/// 阶段确认面板视图。
pub(crate) struct PhaseConfirmationView {
    phase_name: String,
    phase_description: String,
    artifact_preview: String,
    /// 当前选择的选项
    selected_index: usize,
    /// 选项列表
    options: Vec<ConfirmationOption>,
    /// 用户输入的补充内容（当选择 Supplement 时）
    supplement_input: String,
    /// 当前模式：选择 / 输入
    mode: ConfirmMode,
    /// 完成状态
    completion: Option<ViewCompletion>,
    /// 用户选择的动作
    selected_action: Option<UserAction>,
    /// 用于发送命令的事件发送器
    app_event_tx: crate::app_event_sender::AppEventSender,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmMode {
    /// 选择确认/补充
    Select,
    /// 输入补充内容
    Input,
}

#[derive(Debug, Clone)]
struct ConfirmationOption {
    label: String,
    description: String,
    action: UserAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UserAction {
    /// 确认继续到下一阶段
    Continue,
    /// 需要补充内容，包含用户输入
    Supplement(String),
}

impl PhaseConfirmationView {
    pub(crate) fn new(
        phase_name: impl Into<String>,
        phase_description: impl Into<String>,
        artifact_preview: impl Into<String>,
        app_event_tx: crate::app_event_sender::AppEventSender,
    ) -> Self {
        let phase_name = phase_name.into();
        let phase_description = phase_description.into();
        let artifact_preview = artifact_preview.into();

        let options = vec![
            ConfirmationOption {
                label: "✓ 确认继续".to_string(),
                description: "阶段分析已完成，进入下一阶段".to_string(),
                action: UserAction::Continue,
            },
            ConfirmationOption {
                label: "✎ 补充内容".to_string(),
                description: "需要添加说明或修正当前分析".to_string(),
                action: UserAction::Supplement(String::new()),
            },
        ];

        Self {
            phase_name,
            phase_description,
            artifact_preview,
            selected_index: 0,
            options,
            supplement_input: String::new(),
            mode: ConfirmMode::Select,
            completion: None,
            selected_action: None,
            app_event_tx,
        }
    }

    pub(crate) fn user_action(&self) -> Option<UserAction> {
        self.selected_action.clone()
    }

    fn move_selection(&mut self, delta: isize) {
        if self.mode != ConfirmMode::Select {
            return;
        }
        let len = self.options.len() as isize;
        let new_index = (self.selected_index as isize + delta).rem_euclid(len);
        self.selected_index = new_index as usize;
    }

    fn confirm_selection(&mut self) {
        use crate::app_event::AppEvent;
        use crate::zmission_command::Command;

        match self.mode {
            ConfirmMode::Select => {
                let option = &self.options[self.selected_index];
                match &option.action {
                    UserAction::Continue => {
                        self.selected_action = Some(UserAction::Continue);
                        self.completion = Some(ViewCompletion::Accepted);
                        // 自动发送 Continue 命令
                        let _ = self
                            .app_event_tx
                            .send(AppEvent::ZmissionCommand(Command::Continue { note: None }));
                    }
                    UserAction::Supplement(_) => {
                        // 切换到输入模式
                        self.mode = ConfirmMode::Input;
                    }
                }
            }
            ConfirmMode::Input => {
                let content = self.supplement_input.trim().to_string();
                if !content.is_empty() {
                    self.selected_action = Some(UserAction::Supplement(content.clone()));
                    self.completion = Some(ViewCompletion::Accepted);
                    // 自动发送 Continue 命令，附带补充内容
                    let _ = self
                        .app_event_tx
                        .send(AppEvent::ZmissionCommand(Command::Continue {
                            note: Some(content),
                        }));
                }
            }
        }
    }

    fn cancel_input(&mut self) {
        if self.mode == ConfirmMode::Input {
            self.mode = ConfirmMode::Select;
            self.supplement_input.clear();
        } else {
            self.completion = Some(ViewCompletion::Cancelled);
        }
    }

    fn append_to_input(&mut self, c: char) {
        if self.mode == ConfirmMode::Input {
            self.supplement_input.push(c);
        }
    }

    fn backspace_input(&mut self) {
        if self.mode == ConfirmMode::Input {
            self.supplement_input.pop();
        }
    }

    fn body_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let inner_width = width.saturating_sub(4);

        // 阶段信息
        lines.push(Line::from(vec![
            Span::from(format!("阶段：{}", self.phase_name)).bold(),
        ]));
        lines.push(Line::from(vec![
            Span::from(self.phase_description.clone()).dim(),
        ]));
        lines.push(Line::from(""));

        // 产物预览
        lines.push(Line::from(vec![Span::from("产物预览：").bold()]));
        let preview = if self.artifact_preview.len() > inner_width * 5 {
            format!("{}...", &self.artifact_preview[..inner_width * 5])
        } else {
            self.artifact_preview.clone()
        };
        for line in preview.lines() {
            lines.push(Line::from(vec![Span::from(format!("  {}", line)).dim()]));
        }
        lines.push(Line::from(""));

        // 分隔线
        lines.push(Line::from("─".repeat(inner_width).dim()));
        lines.push(Line::from(""));

        // 选项列表
        if self.mode == ConfirmMode::Select {
            lines.push(Line::from(vec![Span::from("请选择操作：").bold()]));
            lines.push(Line::from(""));

            for (i, option) in self.options.iter().enumerate() {
                let is_selected = i == self.selected_index;
                let prefix = if is_selected { "▸ " } else { "  " };
                let label_style = if is_selected {
                    Span::from(option.label.clone()).cyan()
                } else {
                    Span::from(option.label.clone())
                };
                lines.push(Line::from(vec![Span::from(prefix), label_style]));
                if is_selected {
                    lines.push(Line::from(vec![
                        Span::from(format!("    {}", option.description)).dim(),
                    ]));
                }
            }
        } else {
            // 输入模式
            lines.push(Line::from(vec![Span::from("请输入补充内容：").bold()]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::from("> ").cyan(),
                Span::from(self.supplement_input.clone()),
                Span::from("█").cyan().dim(),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::from("Enter 确认 · Esc 返回选择").dim(),
            ]));
        }

        lines
    }
}

impl BottomPaneView for PhaseConfirmationView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Enter => self.confirm_selection(),
            KeyCode::Esc => self.cancel_input(),
            KeyCode::Char(c) => self.append_to_input(c),
            KeyCode::Backspace => self.backspace_input(),
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.completion.is_some()
    }

    fn completion(&self) -> Option<ViewCompletion> {
        self.completion
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.completion = Some(ViewCompletion::Cancelled);
        CancellationEvent::Handled
    }

    fn view_id(&self) -> Option<&'static str> {
        Some("phase-confirmation")
    }
}

impl Renderable for PhaseConfirmationView {
    fn desired_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        let hint_height = 1u16;
        let body_height = Paragraph::new(Text::from(self.body_lines(width as usize)))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(u16::MAX);
        1u16.saturating_add(body_height).saturating_add(hint_height)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let hint_height = if area.height > 1 { 1 } else { 0 };
        let title_height = 1;
        let body_height = area
            .height
            .saturating_sub(title_height)
            .saturating_sub(hint_height);
        let [title_area, body_area, hint_area] = Layout::vertical([
            Constraint::Length(title_height),
            Constraint::Length(body_height),
            Constraint::Length(hint_height),
        ])
        .areas(area);

        Paragraph::new(Line::from("阶段确认".bold())).render(title_area, buf);
        if body_area.height > 0 {
            Paragraph::new(Text::from(self.body_lines(body_area.width as usize)))
                .wrap(Wrap { trim: false })
                .render(body_area, buf);
        }
        if hint_area.height > 0 {
            let hint = if self.mode == ConfirmMode::Select {
                "↑↓ 选择 · Enter 确认 · Esc 取消"
            } else {
                "Enter 确认 · Esc 返回选择"
            };
            Paragraph::new(Line::from(hint.dim())).render(hint_area, buf);
        }
    }
}
