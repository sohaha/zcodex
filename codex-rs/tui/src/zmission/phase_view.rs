//! Phase Agent 视图。
//!
//! 展示当前阶段子代理的界面，包括：
//! - 阶段信息和状态
//! - 子代理消息历史
//! - 快速切换到其他阶段的导航

use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::ViewCompletion;
use crate::render::renderable::Renderable;
use crate::zmission::phase_agent::PhaseAgentMessageSender;
use crate::zmission::PhaseAgentManager;
use codex_mission::MissionPhase;
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
use ratatui::widgets::Wrap;
use ratatui::widgets::Widget;
use std::sync::Arc;
use std::sync::RwLock;

/// Phase Agent 主视图。
pub(crate) struct PhaseAgentView {
    /// 共享的 AgentManager
    manager: Arc<RwLock<PhaseAgentManager>>,
    /// 当前显示的相位（用于导航）
    current_phase: MissionPhase,
    /// 导航模式（显示所有阶段）
    navigation_mode: bool,
    /// 选中的导航索引
    selected_nav_index: usize,
    /// 完成状态
    completion: Option<ViewCompletion>,
}

impl PhaseAgentView {
    pub(crate) fn new(manager: Arc<RwLock<PhaseAgentManager>>) -> Option<Self> {
        let manager_guard = manager.read().ok()?;
        let current_phase = manager_guard.active_agent()?.id.phase;
        drop(manager_guard);

        Some(Self {
            manager,
            current_phase,
            navigation_mode: false,
            selected_nav_index: 0,
            completion: None,
        })
    }

    /// 切换到指定阶段。
    pub(crate) fn switch_to_phase(&mut self, phase: MissionPhase) {
        self.current_phase = phase;
        self.navigation_mode = false;
    }

    fn move_nav_selection(&mut self, delta: isize) {
        if !self.navigation_mode {
            return;
        }
        let len = MissionPhase::ALL.len() as isize;
        let new_index = (self.selected_nav_index as isize + delta).rem_euclid(len);
        self.selected_nav_index = new_index as usize;
    }

    fn confirm_nav_selection(&mut self) {
        if self.navigation_mode {
            let phase = MissionPhase::ALL[self.selected_nav_index];
            self.current_phase = phase;
            self.navigation_mode = false;
        }
    }

    fn toggle_navigation(&mut self) {
        self.navigation_mode = !self.navigation_mode;
        if self.navigation_mode {
            // 设置选中索引为当前阶段
            self.selected_nav_index = self.current_phase.index();
        }
    }

    fn body_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let _inner_width = width.saturating_sub(4);

        let manager_guard = match self.manager.read() {
            Ok(guard) => guard,
            Err(_) => {
                lines.push(Line::from("无法读取 Phase Agent 状态".red()));
                return lines;
            }
        };

        if self.navigation_mode {
            // 导航模式：显示所有阶段列表
            lines.push(Line::from(vec![
                Span::from("阶段导航".bold()),
                Span::from(" (按数字键 1-7 快速跳转)").dim(),
            ]));
            lines.push(Line::from(""));

            for (i, phase) in MissionPhase::ALL.iter().enumerate() {
                let is_selected = i == self.selected_nav_index;
                let is_current = phase == &self.current_phase;
                let is_active = manager_guard.active_agent().map(|a| a.id.phase) == Some(*phase);

                let agent = manager_guard.get_agent(*phase);
                let status = agent.map(|a| a.state.label()).unwrap_or("未知");

                let prefix = if is_selected { "▸ " } else { "  " };
                let number = format!("{}. ", i + 1);

                let name_style = if is_selected {
                    if is_current {
                        Span::from(super::phase_display_name(*phase)).cyan().bold()
                    } else {
                        Span::from(super::phase_display_name(*phase)).cyan()
                    }
                } else if is_current {
                    Span::from(super::phase_display_name(*phase)).bold()
                } else {
                    Span::from(super::phase_display_name(*phase))
                };

                let status_style = match agent.map(|a| &a.state) {
                    Some(super::PhaseAgentState::Completed { .. }) => Span::from(format!("[{}]", status)).green(),
                    Some(super::PhaseAgentState::Running { .. }) => Span::from(format!("[{}]", status)).yellow(),
                    Some(super::PhaseAgentState::AwaitingConfirmation { .. }) => Span::from(format!("[{}]", status)).magenta(),
                    _ => Span::from(format!("[{}]", status)).dim(),
                };

                let mut line_parts = vec![
                    Span::from(prefix),
                    Span::from(number),
                    name_style,
                    Span::from(" "),
                    status_style,
                ];

                if is_active {
                    line_parts.push(Span::from(" ★当前").cyan().dim());
                }

                lines.push(Line::from(line_parts));
            }
        } else {
            // 正常模式：显示当前阶段详情
            let agent = match manager_guard.get_agent(self.current_phase) {
                Some(a) => a,
                None => {
                    lines.push(Line::from("无法获取当前阶段 Agent".red()));
                    return lines;
                }
            };

            // 阶段标题
            let phase_title = format!("{} [{}]", agent.role.display_name(), agent.id.phase.label());
            lines.push(Line::from(vec![
                Span::from(phase_title).bold(),
            ]));

            // 阶段描述
            lines.push(Line::from(vec![
                Span::from(super::phase_description(self.current_phase)).dim(),
            ]));

            // 状态
            let status_text = format!("状态: {}", agent.state.label());
            let status_style = match &agent.state {
                super::PhaseAgentState::Completed { .. } => status_text.green(),
                super::PhaseAgentState::Running { .. } => status_text.yellow(),
                super::PhaseAgentState::AwaitingConfirmation { .. } => status_text.magenta(),
                _ => status_text.into(),
            };
            lines.push(Line::from(vec![status_style]));
            lines.push(Line::from(""));

            // 消息历史
            if agent.messages.is_empty() {
                lines.push(Line::from(vec![
                    Span::from("暂无消息记录").dim(),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::from("对话记录:").bold(),
                ]));

                for msg in &agent.messages {
                    let style = match msg.sender {
                        PhaseAgentMessageSender::Orchestrator => "主代理".cyan(),
                        PhaseAgentMessageSender::Agent => "子代理".green(),
                        PhaseAgentMessageSender::User => "用户".yellow(),
                    };

                    lines.push(Line::from(vec![
                        style,
                        Span::from(format!(": {}", msg.content)).dim(),
                    ]));
                }
            }

            // 补充内容提示
            if !agent.user_supplements.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::from(format!("有 {} 条用户补充内容", agent.user_supplements.len())).cyan(),
                ]));
            }
        }

        lines
    }
}

impl BottomPaneView for PhaseAgentView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => self.move_nav_selection(-1),
            KeyCode::Down => self.move_nav_selection(1),
            KeyCode::Enter => self.confirm_nav_selection(),
            KeyCode::Esc => {
                if self.navigation_mode {
                    self.navigation_mode = false;
                } else {
                    self.completion = Some(ViewCompletion::Cancelled);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => self.toggle_navigation(),
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                let digit = c.to_digit(10).unwrap() as usize;
                if digit <= MissionPhase::ALL.len() {
                    self.switch_to_phase(MissionPhase::ALL[digit - 1]);
                }
            }
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
        Some("phase-agent-view")
    }
}

impl Renderable for PhaseAgentView {
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

        let title = if self.navigation_mode {
            "阶段导航"
        } else {
            "Phase Agent"
        };

        Paragraph::new(Line::from(title.bold()))
            .render(title_area, buf);

        if body_area.height > 0 {
            Paragraph::new(Text::from(self.body_lines(body_area.width as usize)))
                .wrap(Wrap { trim: false })
                .render(body_area, buf);
        }

        if hint_area.height > 0 {
            let hint = if self.navigation_mode {
                "↑↓ 选择 · Enter 确认 · Esc 关闭导航 · 1-7 快速跳转"
            } else {
                "N 打开导航 · 1-7 跳转阶段 · Esc 关闭"
            };
            Paragraph::new(Line::from(hint.dim()))
                .render(hint_area, buf);
        }
    }
}
