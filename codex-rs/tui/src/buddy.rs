use std::time::Duration;
use std::time::Instant;

use rand::Rng;
use rand::SeedableRng;
use rand::prelude::IndexedRandom;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

use crate::live_wrap::take_prefix_by_width;
use crate::render::renderable::Renderable;

const PET_FEEDBACK_DURATION: Duration = Duration::from_millis(2500);

const CAT_NAMES: &[&str] = &["Mochi", "Pixel", "Pico", "Nori", "Miso"];
const DOG_NAMES: &[&str] = &["Biscuit", "Scout", "Poppy", "Tango", "Nugget"];
const FOX_NAMES: &[&str] = &["Ember", "Sable", "Maple", "Juniper", "Vixen"];
const OTTER_NAMES: &[&str] = &["Ripple", "Pebble", "Kelp", "Drift", "Sunny"];
const RABBIT_NAMES: &[&str] = &["Clover", "Pip", "Thistle", "Velvet", "Sprout"];

const CAT_PET_LINES: &[&str] = &[
    "leans into the scritches",
    "purrs loud enough to shake the footer",
    "blinks very slowly at you",
];
const DOG_PET_LINES: &[&str] = &[
    "thumps a happy tail against the terminal",
    "spins once and sits back down",
    "looks thrilled with the attention",
];
const FOX_PET_LINES: &[&str] = &[
    "flicks its tail and preens",
    "accepts the pat with suspicious delight",
    "acts aloof for two seconds, then melts",
];
const OTTER_PET_LINES: &[&str] = &[
    "does a tiny splashy wiggle",
    "offers you a perfectly smooth pebble",
    "floats on its back, fully content",
];
const RABBIT_PET_LINES: &[&str] = &[
    "does a tiny victory hop",
    "tucks in close and settles",
    "wiggles its nose approvingly",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuddySpecies {
    Cat,
    Dog,
    Fox,
    Otter,
    Rabbit,
}

impl BuddySpecies {
    fn label(self) -> &'static str {
        match self {
            Self::Cat => "cat",
            Self::Dog => "dog",
            Self::Fox => "fox",
            Self::Otter => "otter",
            Self::Rabbit => "rabbit",
        }
    }

    fn idle_face(self) -> &'static str {
        match self {
            Self::Cat => "(=^.^=)",
            Self::Dog => "(Uo_x_o)",
            Self::Fox => "(/\\^o^/\\\\)",
            Self::Otter => "(o3o)",
            Self::Rabbit => "(\\\\_//)",
        }
    }

    fn pet_face(self) -> &'static str {
        match self {
            Self::Cat => "(=^w^=)",
            Self::Dog => "(U^x^U)",
            Self::Fox => "(/\\^w^/\\\\)",
            Self::Otter => "(o^^o)",
            Self::Rabbit => "(\\\\^_^//)",
        }
    }

    fn names(self) -> &'static [&'static str] {
        match self {
            Self::Cat => CAT_NAMES,
            Self::Dog => DOG_NAMES,
            Self::Fox => FOX_NAMES,
            Self::Otter => OTTER_NAMES,
            Self::Rabbit => RABBIT_NAMES,
        }
    }

    fn pet_line(self) -> &'static [&'static str] {
        match self {
            Self::Cat => CAT_PET_LINES,
            Self::Dog => DOG_PET_LINES,
            Self::Fox => FOX_PET_LINES,
            Self::Otter => OTTER_PET_LINES,
            Self::Rabbit => RABBIT_PET_LINES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuddyRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl BuddyRarity {
    fn label(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
            Self::Legendary => "legendary",
        }
    }

    fn styled_span(self) -> Span<'static> {
        match self {
            Self::Common => self.label().dim(),
            Self::Uncommon => self.label().green(),
            Self::Rare => self.label().cyan(),
            Self::Epic => self.label().magenta(),
            Self::Legendary => self.label().yellow(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BuddyProfile {
    name: String,
    species: BuddySpecies,
    rarity: BuddyRarity,
}

impl BuddyProfile {
    fn from_seed(seed: &str) -> Self {
        let mut rng = rand::rngs::StdRng::seed_from_u64(stable_seed(seed));
        let rarity = match rng.random_range(0..100) {
            0..=49 => BuddyRarity::Common,
            50..=77 => BuddyRarity::Uncommon,
            78..=91 => BuddyRarity::Rare,
            92..=98 => BuddyRarity::Epic,
            _ => BuddyRarity::Legendary,
        };
        let species = *[
            BuddySpecies::Cat,
            BuddySpecies::Dog,
            BuddySpecies::Fox,
            BuddySpecies::Otter,
            BuddySpecies::Rabbit,
        ]
        .choose(&mut rng)
        .expect("species choices should be non-empty");
        let name = species
            .names()
            .choose(&mut rng)
            .expect("species names should be non-empty")
            .to_string();
        Self {
            name,
            species,
            rarity,
        }
    }

    fn short_summary(&self) -> String {
        format!(
            "{} the {} {}",
            self.name,
            self.rarity.label(),
            self.species.label()
        )
    }
}

fn stable_seed(value: &str) -> u64 {
    let mut hash = 1469598103934665603_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuddyCommandResult {
    pub(crate) message: String,
    pub(crate) hint: Option<String>,
}

pub(crate) struct BuddyWidget {
    profile: Option<BuddyProfile>,
    visible: bool,
    pet_feedback_until: Option<Instant>,
    pet_feedback_line: Option<&'static str>,
}

impl BuddyWidget {
    pub(crate) fn new() -> Self {
        Self {
            profile: None,
            visible: false,
            pet_feedback_until: None,
            pet_feedback_line: None,
        }
    }

    pub(crate) fn feedback_duration() -> Duration {
        PET_FEEDBACK_DURATION
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible && self.profile.is_some()
    }

    pub(crate) fn show(&mut self, seed: &str) -> BuddyCommandResult {
        let was_hatched = self.profile.is_some();
        let summary = self.ensure_profile(seed).short_summary();
        self.visible = true;
        let message = if was_hatched {
            format!("Buddy is now visible: {summary}.")
        } else {
            format!("Buddy hatched: {summary}.")
        };
        BuddyCommandResult {
            message,
            hint: Some("Try `/buddy pet` to interact, or `/buddy hide` to dismiss it.".to_string()),
        }
    }

    pub(crate) fn hide(&mut self) -> BuddyCommandResult {
        if self.profile.is_none() {
            return BuddyCommandResult {
                message: "Buddy has not hatched yet.".to_string(),
                hint: Some("Use `/buddy show` to hatch one for this project.".to_string()),
            };
        }
        self.visible = false;
        self.pet_feedback_until = None;
        self.pet_feedback_line = None;
        BuddyCommandResult {
            message: "Buddy hidden.".to_string(),
            hint: Some("Use `/buddy show` to bring it back.".to_string()),
        }
    }

    pub(crate) fn pet(&mut self, seed: &str) -> BuddyCommandResult {
        let species = self.ensure_profile(seed).species;
        let name = self
            .profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .expect("buddy profile should exist after ensure_profile");
        self.visible = true;
        self.pet_feedback_until = Some(Instant::now() + PET_FEEDBACK_DURATION);
        self.pet_feedback_line = species.pet_line().choose(&mut rand::rng()).copied();
        BuddyCommandResult {
            message: format!("You pet {name}. {}", self.pet_feedback_text()),
            hint: None,
        }
    }

    pub(crate) fn status(&self, _seed: &str) -> BuddyCommandResult {
        let Some(profile) = self.profile.as_ref() else {
            return BuddyCommandResult {
                message: "Buddy has not hatched yet.".to_string(),
                hint: Some("Use `/buddy show` to hatch one for this project.".to_string()),
            };
        };
        let visibility = if self.visible { "visible" } else { "hidden" };
        BuddyCommandResult {
            message: format!("Buddy status: {} ({visibility}).", profile.short_summary()),
            hint: Some("Commands: `/buddy show`, `/buddy pet`, `/buddy hide`.".to_string()),
        }
    }

    fn ensure_profile(&mut self, seed: &str) -> &BuddyProfile {
        self.profile
            .get_or_insert_with(|| BuddyProfile::from_seed(seed))
    }

    fn petting(&self) -> bool {
        self.pet_feedback_until
            .is_some_and(|until| Instant::now() < until && self.visible)
    }

    fn pet_feedback_text(&self) -> &'static str {
        self.pet_feedback_line.unwrap_or("It looks delighted.")
    }

    fn render_line(&self, width: u16) -> Option<Line<'static>> {
        if !self.is_visible() || width < 8 {
            return None;
        }
        let profile = self.profile.as_ref()?;
        let face = if self.petting() {
            profile.species.pet_face()
        } else {
            profile.species.idle_face()
        };
        let prefix = if self.petting() { "<3 " } else { "   " };
        let trailing = if self.petting() {
            format!("{} {}", profile.name, self.pet_feedback_text())
        } else {
            format!(
                "{} the {} {}",
                profile.name,
                profile.rarity.label(),
                profile.species.label()
            )
        };
        let plain_text = format!("{prefix}{face} {trailing}");
        let (truncated, _, _) = take_prefix_by_width(&plain_text, width as usize);
        let line = if truncated == plain_text {
            let mut spans = if self.petting() {
                vec![Span::from(prefix).red()]
            } else {
                vec![Span::from(prefix)]
            };
            spans.push(Span::from(face).bold());
            spans.push(" ".into());
            spans.push(Span::from(profile.name.clone()).cyan());
            if self.petting() {
                spans.push(" ".into());
                spans.push(Span::from(self.pet_feedback_text().to_string()).italic());
            } else {
                spans.push(" the ".into());
                spans.push(profile.rarity.styled_span());
                spans.push(" ".into());
                spans.push(Span::from(profile.species.label().to_string()).dim());
            }
            Line::from(spans)
        } else {
            Line::from(truncated)
        };
        Some(line)
    }
}

impl Renderable for BuddyWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let Some(line) = self.render_line(area.width) else {
            return;
        };
        Paragraph::new(vec![line]).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        u16::from(self.render_line(width).is_some())
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
    fn profile_generation_is_stable() {
        assert_eq!(
            BuddyProfile::from_seed("codex-home::project"),
            BuddyProfile::from_seed("codex-home::project")
        );
    }

    #[test]
    fn hidden_buddy_has_no_height() {
        let buddy = BuddyWidget::new();
        assert_eq!(buddy.desired_height(/*width*/ 60), 0);
    }

    #[test]
    fn visible_buddy_snapshot() {
        let mut buddy = BuddyWidget::new();
        let _ = buddy.show("codex-home::project");
        let width = 60;
        let height = buddy.desired_height(width);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        buddy.render(Rect::new(0, 0, width, height), &mut buf);
        assert_snapshot!("buddy_widget_visible", snapshot_buffer(&buf));
    }
}
