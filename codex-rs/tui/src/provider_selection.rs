use std::collections::HashMap;
use std::io;
use std::time::Duration;

use codex_model_provider_info::AMAZON_BEDROCK_PROVIDER_ID;
use codex_model_provider_info::LMSTUDIO_OSS_PROVIDER_ID;
use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::OLLAMA_OSS_PROVIDER_ID;
use codex_model_provider_info::OPENAI_PROVIDER_ID;
use crossterm::event;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize as _;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;

const BUILT_IN_PROVIDER_ORDER: &[&str] = &[
    OPENAI_PROVIDER_ID,
    AMAZON_BEDROCK_PROVIDER_ID,
    OLLAMA_OSS_PROVIDER_ID,
    LMSTUDIO_OSS_PROVIDER_ID,
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderSelectionItem {
    id: String,
    name: Option<String>,
}

impl ProviderSelectionItem {
    fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(self.id.as_str())
    }
}

struct ProviderSelectionWidget {
    items: Vec<ProviderSelectionItem>,
    selected_idx: usize,
}

impl ProviderSelectionWidget {
    fn new(
        providers: &HashMap<String, ModelProviderInfo>,
        current_provider_id: Option<&str>,
    ) -> Self {
        let items = provider_selection_items(providers);
        let selected_idx = current_provider_id
            .and_then(|current| items.iter().position(|item| item.id == current))
            .unwrap_or(0);

        Self {
            items,
            selected_idx,
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Option<io::Result<String>> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        match key.code {
            KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                Some(Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "用户已取消 provider 选择",
                )))
            }
            KeyCode::Esc => Some(Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "用户已取消 provider 选择",
            ))),
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_idx = (self.selected_idx + self.items.len() - 1) % self.items.len();
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_idx = (self.selected_idx + 1) % self.items.len();
                None
            }
            KeyCode::Home => {
                self.selected_idx = 0;
                None
            }
            KeyCode::End => {
                self.selected_idx = self.items.len() - 1;
                None
            }
            KeyCode::Enter => Some(Ok(self.items[self.selected_idx].id.clone())),
            _ => None,
        }
    }
}

impl WidgetRef for &ProviderSelectionWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" 选择模型渠道 ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::Cyan));
        let inner = block.inner(area);
        block.render(area, buf);

        let rows: Vec<Line> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let marker = if idx == self.selected_idx { "›" } else { " " };
                let mut spans = vec![
                    Span::raw(format!("{marker} ")),
                    item.id.clone().cyan().bold(),
                ];
                if item.display_name() != item.id {
                    spans.push("  ".into());
                    spans.push(item.display_name().dim());
                }

                let line = Line::from(spans);
                if idx == self.selected_idx {
                    line.style(Style::new().bg(Color::Cyan).fg(Color::Black))
                } else {
                    line
                }
            })
            .collect();

        let footer = Line::from("↑/↓ 或 j/k 选择，Enter 确认，Esc/Ctrl+C 取消").dim();
        let content = Paragraph::new(rows)
            .block(Block::default())
            .wrap(Wrap { trim: false });

        let [list_area, footer_area] =
            ratatui::layout::Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
                .areas(inner.inner(Margin::new(1, 0)));

        content.render(list_area, buf);
        footer.render(footer_area, buf);
    }
}

pub async fn select_model_provider(
    providers: &HashMap<String, ModelProviderInfo>,
    current_provider_id: Option<&str>,
) -> io::Result<String> {
    if providers.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "没有可用的 model_provider",
        ));
    }

    let mut widget = ProviderSelectionWidget::new(providers, current_provider_id);
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = loop {
        terminal.draw(|f| {
            (&widget).render_ref(f.area(), f.buffer_mut());
        })?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key_event) = event::read()?
            && let Some(selection) = widget.handle_key_event(key_event)
        {
            break selection;
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn provider_selection_items(
    providers: &HashMap<String, ModelProviderInfo>,
) -> Vec<ProviderSelectionItem> {
    let mut items: Vec<_> = providers
        .iter()
        .map(|(id, provider)| ProviderSelectionItem {
            id: id.clone(),
            name: provider.name.clone(),
        })
        .collect();

    items.sort_by(|left, right| {
        provider_sort_key(left.id.as_str()).cmp(&provider_sort_key(right.id.as_str()))
    });
    items
}

fn provider_sort_key(id: &str) -> (usize, &str) {
    let priority = BUILT_IN_PROVIDER_ORDER
        .iter()
        .position(|provider_id| *provider_id == id)
        .unwrap_or(BUILT_IN_PROVIDER_ORDER.len());
    (priority, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_model_provider_info::built_in_model_providers;
    use pretty_assertions::assert_eq;

    #[test]
    fn provider_items_prioritize_builtin_order_then_custom_ids() {
        let mut providers = built_in_model_providers(/*openai_base_url*/ None);
        providers.insert(
            "custom-z".to_string(),
            ModelProviderInfo {
                name: Some("Custom Z".to_string()),
                ..ModelProviderInfo::default()
            },
        );
        providers.insert(
            "custom-a".to_string(),
            ModelProviderInfo {
                name: Some("Custom A".to_string()),
                ..ModelProviderInfo::default()
            },
        );

        let ids = provider_selection_items(&providers)
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "openai".to_string(),
                "amazon-bedrock".to_string(),
                "ollama".to_string(),
                "lmstudio".to_string(),
                "custom-a".to_string(),
                "custom-z".to_string(),
            ]
        );
    }

    #[test]
    fn widget_selects_current_provider_when_available() {
        let providers = built_in_model_providers(/*openai_base_url*/ None);
        let widget = ProviderSelectionWidget::new(&providers, Some("ollama"));

        assert_eq!(widget.items[widget.selected_idx].id, "ollama");
    }
}
