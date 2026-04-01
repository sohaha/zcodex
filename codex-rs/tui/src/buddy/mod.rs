use std::time::Instant;

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
use model::PET_FEEDBACK_DURATION;
use model::REACTION_DURATION;

pub(crate) struct BuddyWidget {
    bones: Option<BuddyBones>,
    state: BuddyState,
}

impl BuddyWidget {
    pub(crate) fn new() -> Self {
        Self {
            bones: None,
            state: BuddyState::default(),
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
        self.state.visible = true;
        if !was_hatched && self.state.reaction.is_none() {
            let now = Instant::now();
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
        let now = Instant::now();
        self.state.visible = true;
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
                "Buddy is back: {} {}.",
                bones.short_summary(),
                bones.rarity.stars()
            )
        } else {
            format!(
                "Buddy hatched: {} {}.",
                bones.short_summary(),
                bones.rarity.stars()
            )
        };
        BuddyCommandResult {
            message,
            hint: Some("Try `/buddy pet` to interact, or `/buddy status` for traits.".to_string()),
        }
    }

    pub(crate) fn hide(&mut self) -> BuddyCommandResult {
        let Some(bones) = self.bones.as_ref() else {
            return BuddyCommandResult {
                message: "Buddy has not hatched yet.".to_string(),
                hint: Some("Use `/buddy show` to hatch one for this project.".to_string()),
            };
        };
        self.state.visible = false;
        self.state.pet_started_at = None;
        self.state.pet_until = None;
        self.state.reaction = None;
        self.state.last_action = Some(BuddyLastAction::Hidden);
        BuddyCommandResult {
            message: format!("Buddy hidden: {}.", bones.short_summary()),
            hint: Some("Use `/buddy show` to bring it back.".to_string()),
        }
    }

    pub(crate) fn pet(&mut self, seed: &str) -> BuddyCommandResult {
        let bones = self.ensure_bones(seed).clone();
        let now = Instant::now();
        self.state.visible = true;
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
            message: format!("You pet {}. {}", bones.name, reaction_text),
            hint: Some("Use `/buddy status` to inspect rarity, traits, and mood.".to_string()),
        }
    }

    pub(crate) fn status(&self, _seed: &str) -> BuddyCommandResult {
        let Some(bones) = self.bones.as_ref() else {
            return BuddyCommandResult {
                message: "Buddy has not hatched yet.".to_string(),
                hint: Some("Use `/buddy show` to hatch one for this project.".to_string()),
            };
        };

        let visibility = if self.state.visible {
            "visible"
        } else {
            "hidden"
        };
        let (primary_stat, primary_value) = bones.stats.primary();
        let mood = if self.state.is_petting() {
            "delighted"
        } else if let Some(reaction) = self.state.active_reaction() {
            match reaction.kind {
                BuddyReactionKind::Hatch => "freshly hatched",
                BuddyReactionKind::Return => "settled back in",
                BuddyReactionKind::Pet => "very pleased",
                BuddyReactionKind::Teaser => "waiting for attention",
            }
        } else if self.state.visible {
            "alert"
        } else {
            "resting offstage"
        };
        let shiny = if bones.shiny { ", shiny" } else { "" };
        let message = format!(
            "Buddy status: {} {} ({visibility}{shiny}, {}, {} eyes, mood {mood}, pets {}). Peak stat: {} {}.",
            bones.short_summary(),
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
                "Commands: `/buddy show`, `/buddy pet`, `/buddy hide`, `/buddy status`."
                    .to_string(),
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
        render::render_lines(bones, &self.state, width)
    }
}

fn reaction_text(bones: &BuddyBones, kind: BuddyReactionKind, index: u32) -> &'static str {
    let lines = match kind {
        BuddyReactionKind::Hatch => bones.species.hatch_lines(),
        BuddyReactionKind::Return => bones.species.return_lines(),
        BuddyReactionKind::Pet => bones.species.pet_lines(),
        BuddyReactionKind::Teaser => bones.species.teaser_lines(),
    };
    lines[index as usize % lines.len()]
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
        assert!(status.message.contains("Peak stat:"));
        assert!(status.message.contains("visible"));
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
}
