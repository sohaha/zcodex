use std::time::Instant;

use codex_config::types::BuddySoul;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use crate::render::renderable::Renderable;

mod model;
mod render;

use model::BuddyBones;
pub(crate) use model::BuddyCommandResult;
use model::BuddyLastAction;
use model::BuddyReaction;
use model::BuddyReactionKind;
use model::BuddyState;
use model::FULL_LAYOUT_INTRO_DURATION;
use model::PET_FEEDBACK_DURATION;
use model::REACTION_DURATION;

pub(crate) struct BuddyWidget {
    bones: Option<BuddyBones>,
    state: BuddyState,
    soul: Option<BuddySoul>,
}

impl BuddyWidget {
    pub(crate) fn new() -> Self {
        Self {
            bones: None,
            state: BuddyState::default(),
            soul: None,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.state.visible && self.bones.is_some()
    }

    pub(crate) fn next_redraw_in(&self) -> Option<std::time::Duration> {
        self.state.next_redraw_in()
    }

    pub(crate) fn ensure_visible(&mut self, seed: &str) {
        let was_hatched = self.bones.is_some();
        let bones = self.ensure_bones(seed).clone();
        let now = Instant::now();
        self.show_temporary_full(now);
        if !was_hatched && self.state.reaction.is_none() {
            self.state.reaction = Some(BuddyReaction {
                kind: BuddyReactionKind::Teaser,
                text: reaction_text(&bones, BuddyReactionKind::Teaser, /*index*/ 0).to_string(),
                until: now + REACTION_DURATION,
            });
        }
    }

    pub(crate) fn show(&mut self, seed: &str) -> BuddyCommandResult {
        let was_hatched = self.bones.is_some();
        let bones = self.ensure_bones(seed).clone();
        let name = self.display_name(&bones).to_string();
        let now = Instant::now();
        self.show_temporary_full(now);
        let reaction_kind = if was_hatched {
            BuddyReactionKind::Return
        } else {
            BuddyReactionKind::Hatch
        };
        let reaction_text = reaction_text(&bones, reaction_kind, self.state.pet_count);
        self.state.reaction = Some(BuddyReaction {
            kind: reaction_kind,
            text: reaction_text.to_string(),
            until: now + REACTION_DURATION,
        });
        self.state.last_action = Some(if was_hatched {
            BuddyLastAction::Reappeared
        } else {
            BuddyLastAction::Hatched
        });

        let message = if was_hatched {
            format!(
                "小伙伴回来了：{} {}。",
                short_summary_with_name(&bones, &name),
                bones.rarity.stars()
            )
        } else {
            format!(
                "小伙伴已孵化：{} {}。",
                short_summary_with_name(&bones, &name),
                bones.rarity.stars()
            )
        };
        BuddyCommandResult {
            message,
            hint: Some("试试 `/buddy pet` 来互动，或用 `/buddy status` 查看特征。".to_string()),
        }
    }

    pub(crate) fn show_full(&mut self, seed: &str) -> BuddyCommandResult {
        let was_hatched = self.bones.is_some();
        let bones = self.ensure_bones(seed).clone();
        let name = self.display_name(&bones).to_string();
        let now = Instant::now();
        self.state.visible = true;
        self.state.full_layout = true;
        self.state.full_layout_until = None;
        let reaction_kind = if was_hatched {
            BuddyReactionKind::Return
        } else {
            BuddyReactionKind::Hatch
        };
        let reaction_text = reaction_text(&bones, reaction_kind, self.state.pet_count);
        self.state.reaction = Some(BuddyReaction {
            kind: reaction_kind,
            text: reaction_text.to_string(),
            until: now + REACTION_DURATION,
        });
        self.state.last_action = Some(if was_hatched {
            BuddyLastAction::Reappeared
        } else {
            BuddyLastAction::Hatched
        });

        BuddyCommandResult {
            message: format!(
                "小伙伴进入全形象常驻：{}。",
                short_summary_with_name(&bones, &name)
            ),
            hint: Some(
                "使用 `/buddy hide` 完全隐藏，或用 `/buddy status` 查看当前状态。".to_string(),
            ),
        }
    }

    pub(crate) fn hide(&mut self) -> BuddyCommandResult {
        let Some(bones) = self.bones.as_ref() else {
            return BuddyCommandResult {
                message: "小伙伴还没孵化。".to_string(),
                hint: Some("使用 `/buddy show` 为此项目孵化一个。".to_string()),
            };
        };
        let name = self.display_name(bones).to_string();
        self.state.visible = false;
        self.state.full_layout = false;
        self.state.full_layout_until = None;
        self.state.pet_started_at = None;
        self.state.pet_until = None;
        self.state.reaction = None;
        self.state.last_action = Some(BuddyLastAction::Hidden);
        BuddyCommandResult {
            message: format!("小伙伴已隐藏：{}。", short_summary_with_name(bones, &name)),
            hint: Some("使用 `/buddy show` 让它回来。".to_string()),
        }
    }

    pub(crate) fn pet(&mut self, seed: &str) -> BuddyCommandResult {
        let bones = self.ensure_bones(seed).clone();
        let name = self.display_name(&bones).to_string();
        let now = Instant::now();
        self.show_temporary_full(now);
        self.state.pet_count += 1;
        self.state.pet_started_at = Some(now);
        self.state.pet_until = Some(now + PET_FEEDBACK_DURATION);
        let reaction_text = reaction_text(&bones, BuddyReactionKind::Pet, self.state.pet_count - 1);
        self.state.reaction = Some(BuddyReaction {
            kind: BuddyReactionKind::Pet,
            text: reaction_text.to_string(),
            until: now + REACTION_DURATION,
        });
        self.state.last_action = Some(BuddyLastAction::Petted);

        BuddyCommandResult {
            message: format!("你抚摸了 {name}。{reaction_text}"),
            hint: Some("用 `/buddy status` 查看稀有度、特征和心情。".to_string()),
        }
    }

    pub(crate) fn status(&self, _seed: &str) -> BuddyCommandResult {
        let Some(bones) = self.bones.as_ref() else {
            return BuddyCommandResult {
                message: "小伙伴还没孵化。".to_string(),
                hint: Some("使用 `/buddy show` 为此项目孵化一个。".to_string()),
            };
        };
        let name = self.display_name(bones);

        let (primary_stat, primary_value) = bones.stats.primary();
        let mood = if self.state.is_petting() {
            "开心"
        } else if let Some(reaction) = self.state.active_reaction() {
            match reaction.kind {
                BuddyReactionKind::Hatch => "刚孵化",
                BuddyReactionKind::Return => "安顿好了",
                BuddyReactionKind::Pet => "很满意",
                BuddyReactionKind::Teaser => "等你关注",
                BuddyReactionKind::Observe => "在观察",
            }
        } else if self.state.visible {
            "警觉"
        } else {
            "幕后休息"
        };
        let visibility = if self.state.visible {
            "可见"
        } else {
            "隐藏"
        };
        let display_mode = if self.state.full_layout_active() {
            "全形象"
        } else {
            "紧凑"
        };
        let shiny = if bones.shiny { "，闪亮" } else { "" };
        let visual_trait = bones.rarity.visual_trait();
        let personality = self
            .soul
            .as_ref()
            .map(|soul| format!(" 性格：{}。", soul.personality))
            .unwrap_or_default();
        let message = format!(
            "小伙伴状态：{} {}（{visibility}，{display_mode}{shiny}，{}，{}眼，{visual_trait}，心情{mood}，抚摸 {}）。峰值属性：{} {}。{personality}",
            short_summary_with_name(bones, name),
            bones.rarity.stars(),
            bones.hat.label(),
            bones.eye.label(),
            self.state.pet_count,
            primary_stat.label(),
            primary_value
        );
        BuddyCommandResult {
            message,
            hint: Some(
                "命令：`/buddy show`、`/buddy full`、`/buddy pet`、`/buddy hide`、`/buddy status`。".to_string(),
            ),
        }
    }

    fn ensure_bones(&mut self, seed: &str) -> &BuddyBones {
        self.bones
            .get_or_insert_with(|| BuddyBones::from_seed(seed))
    }

    fn render_lines(&self, width: u16) -> Vec<ratatui::text::Line<'static>> {
        let Some(bones) = self.bones.as_ref() else {
            return Vec::new();
        };
        if !self.is_visible() {
            return Vec::new();
        }
        let name = self.display_name(bones);
        let render_mode = if self.state.full_layout_active() && width >= render::full_layout_width()
        {
            render::BuddyRenderMode::Full
        } else {
            render::BuddyRenderMode::Compact
        };
        render::render_lines(bones, name, &self.state, width, render_mode)
    }

    pub(crate) fn bubble_lines(&self, width: u16) -> Vec<ratatui::text::Line<'static>> {
        let Some(bones) = self.bones.as_ref() else {
            return Vec::new();
        };
        if !self.is_visible()
            || !self.state.full_layout_active()
            || width < render::full_layout_width()
        {
            return Vec::new();
        }
        render::render_bubble_lines(bones, &self.state, width)
    }

    pub(crate) fn set_soul(&mut self, soul: Option<BuddySoul>) -> bool {
        if self.soul == soul {
            return false;
        }
        self.soul = soul;
        true
    }

    pub(crate) fn react(&mut self, seed: &str, text: String) -> bool {
        if !self.state.visible {
            return false;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return false;
        }
        let _ = self.ensure_bones(seed);
        let now = Instant::now();
        self.state.reaction = Some(BuddyReaction {
            kind: BuddyReactionKind::Observe,
            text: trimmed.to_string(),
            until: now + REACTION_DURATION,
        });
        self.state.last_action = Some(BuddyLastAction::Observed);
        true
    }

    fn display_name<'a>(&'a self, bones: &'a BuddyBones) -> &'a str {
        self.soul
            .as_ref()
            .map(|soul| soul.name.as_str())
            .unwrap_or(bones.name.as_str())
    }

    fn show_temporary_full(&mut self, now: Instant) {
        self.state.visible = true;
        if self.state.full_layout {
            self.state.full_layout_until = None;
            return;
        }
        self.state.full_layout = false;
        self.state.full_layout_until = Some(now + FULL_LAYOUT_INTRO_DURATION);
    }
}

fn reaction_text(bones: &BuddyBones, kind: BuddyReactionKind, index: u32) -> &'static str {
    let lines = match kind {
        BuddyReactionKind::Hatch => bones.species.hatch_lines(),
        BuddyReactionKind::Return => bones.species.return_lines(),
        BuddyReactionKind::Pet => bones.species.pet_lines(),
        BuddyReactionKind::Teaser | BuddyReactionKind::Observe => bones.species.teaser_lines(),
    };
    lines[index as usize % lines.len()]
}

fn short_summary_with_name(bones: &BuddyBones, name: &str) -> String {
    format!(
        "{} the {} {}",
        name,
        bones.rarity.label(),
        bones.species.label()
    )
}

impl Renderable for BuddyWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let lines = self.render_lines(area.width);
        if lines.is_empty() {
            return;
        }
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.render_lines(width).len() as u16
    }
}

pub(crate) struct BuddyBubble<'a> {
    buddy: &'a BuddyWidget,
}

impl<'a> BuddyBubble<'a> {
    pub(crate) fn new(buddy: &'a BuddyWidget) -> Self {
        Self { buddy }
    }
}

impl Renderable for BuddyBubble<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let lines = self.buddy.bubble_lines(area.width);
        if lines.is_empty() {
            return;
        }
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.buddy.bubble_lines(width).len() as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;

    fn snapshot_buffer(buf: &Buffer) -> String {
        let mut lines = Vec::new();
        for y in 0..buf.area().height {
            let mut row = String::new();
            for x in 0..buf.area().width {
                row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            lines.push(row.trim_end().to_string());
        }
        lines.join("\n")
    }

    #[test]
    fn bones_generation_is_stable() {
        assert_eq!(
            BuddyBones::from_seed("codex-home::project"),
            BuddyBones::from_seed("codex-home::project")
        );
    }

    #[test]
    fn hidden_buddy_has_no_height() {
        let buddy = BuddyWidget::new();
        assert_eq!(buddy.desired_height(/*width*/ 60), 0);
    }

    #[test]
    fn buddy_status_reports_peak_stat_and_visibility() {
        let mut buddy = BuddyWidget::new();
        let _ = buddy.show("codex-home::project");
        let status = buddy.status("codex-home::project");
        assert!(status.message.contains("峰值属性："));
        assert!(status.message.contains("可见"));
        assert_eq!(
            status.hint,
            Some(
                "命令：`/buddy show`、`/buddy full`、`/buddy pet`、`/buddy hide`、`/buddy status`。"
                    .to_string()
            )
        );
    }

    #[test]
    fn visible_buddy_wide_snapshot() {
        let mut buddy = BuddyWidget::new();
        let _ = buddy.show("codex-home::project");
        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_visible_wide", snapshot_buffer(&buf));
    }

    #[test]
    fn buddy_bubble_snapshot() {
        let mut buddy = BuddyWidget::new();
        let _ = buddy.show("codex-home::project");
        let width = 60;
        let bubble = BuddyBubble::new(&buddy);
        let height = bubble.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        bubble.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_bubble", snapshot_buffer(&buf));
    }

    #[test]
    fn startup_teaser_snapshot() {
        let mut buddy = BuddyWidget::new();
        buddy.ensure_visible("codex-home::project");
        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_startup_teaser", snapshot_buffer(&buf));
    }

    #[test]
    fn wide_buddy_compacts_after_intro_expires() {
        let mut buddy = BuddyWidget::new();
        buddy.ensure_visible("codex-home::project");
        buddy.state.full_layout_until = Some(Instant::now() - model::TICK_DURATION);
        buddy.state.reaction = None;

        let width = 60;
        let height = buddy.desired_height(width);
        assert_eq!(height, 1);
        assert_eq!(BuddyBubble::new(&buddy).desired_height(width), 0);

        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!(
            "buddy_widget_compact_after_intro_wide",
            snapshot_buffer(&buf)
        );
    }

    #[test]
    fn goose_buddy_full_snapshot() {
        let mut buddy = BuddyWidget::new();
        let mut bones = BuddyBones::from_seed("codex-goose::project");
        bones.species = model::BuddySpecies::Goose;
        bones.name = "Honk".to_string();
        buddy.bones = Some(bones);
        buddy.state.visible = true;
        buddy.state.full_layout = true;

        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_goose_full", snapshot_buffer(&buf));
    }

    #[test]
    fn snail_buddy_full_snapshot() {
        let mut buddy = BuddyWidget::new();
        let mut bones = BuddyBones::from_seed("codex-snail::project");
        bones.species = model::BuddySpecies::Snail;
        bones.name = "Shelly".to_string();
        buddy.bones = Some(bones);
        buddy.state.visible = true;
        buddy.state.full_layout = true;

        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_snail_full", snapshot_buffer(&buf));
    }

    #[test]
    fn visible_buddy_narrow_snapshot() {
        let mut buddy = BuddyWidget::new();
        let _ = buddy.show("codex-home::project");
        let width = 30;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_visible_narrow", snapshot_buffer(&buf));
    }

    #[test]
    fn petted_buddy_snapshot() {
        let mut buddy = BuddyWidget::new();
        let _ = buddy.pet("codex-home::project");
        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_petted", snapshot_buffer(&buf));
    }

    #[test]
    fn visible_buddy_keeps_requesting_animation_frames() {
        let mut buddy = BuddyWidget::new();
        buddy.ensure_visible("codex-home::project");
        assert!(buddy.next_redraw_in().is_some());
    }

    #[test]
    fn visible_buddy_idle_frames_change_over_time() {
        let mut buddy = BuddyWidget::new();
        buddy.ensure_visible("codex-home::project");
        let start = Instant::now();
        let first = buddy.state.frame_at(start);
        let second = buddy.state.frame_at(start + model::TICK_DURATION);
        let third = buddy.state.frame_at(start + model::TICK_DURATION * 2);
        assert_ne!(first, second);
        assert_ne!(second, third);
    }

    #[test]
    fn uncommon_buddy_full_snapshot() {
        let mut buddy = BuddyWidget::new();
        let mut bones = BuddyBones::from_seed("uncommon-test::project");
        bones.rarity = model::BuddyRarity::Uncommon;
        bones.species = model::BuddySpecies::Cat;
        bones.name = "年糕".to_string();
        buddy.bones = Some(bones);
        buddy.state.visible = true;
        buddy.state.full_layout = true;

        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_uncommon_full", snapshot_buffer(&buf));
    }

    #[test]
    fn epic_buddy_full_snapshot() {
        let mut buddy = BuddyWidget::new();
        let mut bones = BuddyBones::from_seed("epic-test::project");
        bones.rarity = model::BuddyRarity::Epic;
        bones.species = model::BuddySpecies::Dragon;
        bones.name = "Cobalt".to_string();
        buddy.bones = Some(bones);
        buddy.state.visible = true;
        buddy.state.full_layout = true;

        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_epic_full", snapshot_buffer(&buf));
    }

    #[test]
    fn legendary_buddy_full_snapshot() {
        let mut buddy = BuddyWidget::new();
        let mut bones = BuddyBones::from_seed("legendary-test::project");
        bones.rarity = model::BuddyRarity::Legendary;
        bones.species = model::BuddySpecies::Fox;
        bones.name = "Ember".to_string();
        buddy.bones = Some(bones);
        buddy.state.visible = true;
        buddy.state.full_layout = true;

        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_legendary_full", snapshot_buffer(&buf));
    }

    #[test]
    fn legendary_buddy_narrow_snapshot() {
        let mut buddy = BuddyWidget::new();
        let mut bones = BuddyBones::from_seed("legendary-narrow::project");
        bones.rarity = model::BuddyRarity::Legendary;
        bones.species = model::BuddySpecies::Fox;
        bones.name = "Ember".to_string();
        buddy.bones = Some(bones);
        buddy.state.visible = true;
        buddy.state.full_layout = false;

        let width = 30;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_legendary_narrow", snapshot_buffer(&buf));
    }
}
