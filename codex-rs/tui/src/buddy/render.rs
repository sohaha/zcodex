use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use textwrap::Options;

use crate::live_wrap::take_prefix_by_width;

use super::model::BuddyBones;
use super::model::BuddyHat;
use super::model::BuddyState;

const MIN_RENDER_WIDTH: u16 = 12;
const FULL_LAYOUT_WIDTH: u16 = 46;
const MAX_BUBBLE_WIDTH: usize = 30;

pub(crate) fn render_lines(
    bones: &BuddyBones,
    state: &BuddyState,
    width: u16,
) -> Vec<Line<'static>> {
    if width < MIN_RENDER_WIDTH {
        return Vec::new();
    }

    if width < FULL_LAYOUT_WIDTH {
        return vec![render_narrow_line(bones, state, width)];
    }

    let mut lines = Vec::new();
    if let Some(text) = state.active_reaction_text() {
        lines.extend(render_bubble(text, width));
    }
    if state.is_petting() {
        lines.push(vec!["  ".into(), "<3".red(), "   ".into(), "<3".red()].into());
    }

    for sprite_line in sprite_lines(bones, state) {
        lines.push(Line::from(vec![Span::styled(
            sprite_line,
            rarity_style(bones),
        )]));
    }

    lines.push(render_identity_line(bones, state));
    lines.push(render_traits_line(bones, state, width));
    lines
}

fn render_narrow_line(bones: &BuddyBones, state: &BuddyState, width: u16) -> Line<'static> {
    let label = state
        .active_reaction_text()
        .map(|text| format!("\"{text}\""))
        .unwrap_or_else(|| {
            let shiny = if bones.shiny { " ✦" } else { "" };
            format!(
                "{} {} {}{}",
                bones.name,
                bones.rarity.stars(),
                bones.species.label(),
                shiny
            )
        });
    let face = mini_face(bones, state);
    let prefix = if state.is_petting() { "<3 " } else { "" };
    let plain = format!("{prefix}{face} {label}");
    let truncated = truncate_with_ellipsis(&plain, width);

    if truncated != plain {
        return Line::from(truncated);
    }

    let mut spans = Vec::new();
    if state.is_petting() {
        spans.push("<3 ".red());
    }
    spans.push(Span::styled(
        face.to_string(),
        rarity_style(bones).add_modifier(ratatui::style::Modifier::BOLD),
    ));
    spans.push(" ".into());
    spans.push(bones.name.clone().cyan());
    if let Some(text) = state.active_reaction_text() {
        spans.push(" ".into());
        spans.push(format!("\"{text}\"").italic());
    } else {
        spans.push(" ".into());
        spans.push(bones.rarity.stars_span());
        spans.push(" ".into());
        spans.push(bones.species.label().dim());
        if bones.shiny {
            spans.push(" ✦".magenta().bold());
        }
    }
    Line::from(spans)
}

fn render_bubble(text: &str, width: u16) -> Vec<Line<'static>> {
    let bubble_width = usize::from(width.saturating_sub(6)).clamp(16, MAX_BUBBLE_WIDTH);
    let wrapped = textwrap::wrap(text, Options::new(bubble_width));
    wrapped
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let prefix = if index == 0 { "  o " } else { "  | " };
            vec![prefix.dim(), line.to_string().italic()].into()
        })
        .collect()
}

fn render_identity_line(bones: &BuddyBones, state: &BuddyState) -> Line<'static> {
    let visibility = if state.visible { "visible" } else { "hidden" };
    let mut spans = vec![
        "  ".into(),
        bones.name.clone().cyan().bold(),
        " ".into(),
        bones.rarity.stars_span(),
        " ".into(),
        bones.rarity.styled_span(),
        " ".into(),
        bones.species.label().dim(),
        " · ".dim(),
        visibility.dim(),
    ];
    if bones.shiny {
        spans.push(" · ".dim());
        spans.push("shiny".magenta().bold());
    }
    Line::from(spans)
}

fn render_traits_line(bones: &BuddyBones, state: &BuddyState, width: u16) -> Line<'static> {
    let (primary_name, primary_value) = bones.stats.primary();
    let mood = if state.is_petting() {
        "delighted"
    } else if state.visible {
        "alert"
    } else {
        "resting"
    };
    let traits = format!(
        "  peak {} {} · {} · {} eyes · mood {}",
        primary_name.label(),
        primary_value,
        bones.hat.label(),
        bones.eye.label(),
        mood
    );
    let truncated = truncate_with_ellipsis(&traits, width);
    Line::from(truncated.dim())
}

fn sprite_lines(bones: &BuddyBones, state: &BuddyState) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(hat) = hat_line(bones.hat) {
        lines.push(hat.to_string());
    }
    lines.extend(match bones.species {
        super::model::BuddySpecies::Cat => cat_lines(bones, state),
        super::model::BuddySpecies::Fox => fox_lines(bones, state),
        super::model::BuddySpecies::Otter => otter_lines(bones, state),
        super::model::BuddySpecies::Rabbit => rabbit_lines(bones, state),
        super::model::BuddySpecies::Owl => owl_lines(bones, state),
        super::model::BuddySpecies::Dragon => dragon_lines(bones, state),
        super::model::BuddySpecies::Ghost => ghost_lines(bones, state),
        super::model::BuddySpecies::Robot => robot_lines(bones, state),
    });
    lines
}

fn hat_line(hat: BuddyHat) -> Option<&'static str> {
    match hat {
        BuddyHat::None => None,
        BuddyHat::Crown => Some("   _/\\_   "),
        BuddyHat::TopHat => Some("   _____   "),
        BuddyHat::Halo => Some("   .---.   "),
        BuddyHat::Wizard => Some("    /\\\\    "),
        BuddyHat::Beanie => Some("   _____   "),
        BuddyHat::Propeller => Some("  --(*)--  "),
    }
}

fn cat_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        "  /\\_/\\\\  ".to_string(),
        format!(" ( {eye}{eye} ) "),
        "  > ^ <   ".to_string(),
    ]
}

fn fox_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        " /\\   /\\\\ ".to_string(),
        format!("( {eye} v {eye} )"),
        " \\\\_---_//".to_string(),
    ]
}

fn otter_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        "  .-\"\"-.  ".to_string(),
        format!(" / {eye}  {eye} \\\\"),
        " \\\\_====_/".to_string(),
    ]
}

fn rabbit_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        "  (\\ /)   ".to_string(),
        format!(" ( {eye} {eye} ) "),
        " /  ^  \\\\ ".to_string(),
    ]
}

fn owl_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        "  ,_,     ".to_string(),
        format!(" ( {eye} {eye} ) "),
        " /)___(\\\\ ".to_string(),
    ]
}

fn dragon_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        "  /\\_/\\\\  ".to_string(),
        format!(" ( {eye} ~ {eye} )"),
        "  \\\\_v_// ".to_string(),
    ]
}

fn ghost_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        "  .---.   ".to_string(),
        format!(" ( {eye} {eye} ) "),
        " /~ ~~~\\\\ ".to_string(),
    ]
}

fn robot_lines(bones: &BuddyBones, state: &BuddyState) -> [String; 3] {
    let eye = bones.eye.glyph(state.is_petting());
    [
        "  [---]   ".to_string(),
        format!("  | {eye} {eye} | "),
        "  /|___|\\\\ ".to_string(),
    ]
}

fn mini_face(bones: &BuddyBones, state: &BuddyState) -> &'static str {
    let petting = state.is_petting();
    match (bones.species, petting) {
        (super::model::BuddySpecies::Cat, false) => "(=^.^=)",
        (super::model::BuddySpecies::Cat, true) => "(=^w^=)",
        (super::model::BuddySpecies::Fox, false) => "(/\\^.^/\\\\)",
        (super::model::BuddySpecies::Fox, true) => "(/\\^w^/\\\\)",
        (super::model::BuddySpecies::Otter, false) => "(o3o)",
        (super::model::BuddySpecies::Otter, true) => "(o^^o)",
        (super::model::BuddySpecies::Rabbit, false) => "(\\\\_//)",
        (super::model::BuddySpecies::Rabbit, true) => "(\\\\^_^//)",
        (super::model::BuddySpecies::Owl, false) => "(OvO)",
        (super::model::BuddySpecies::Owl, true) => "(OwO)",
        (super::model::BuddySpecies::Dragon, false) => "<:===>",
        (super::model::BuddySpecies::Dragon, true) => "<:^^:>",
        (super::model::BuddySpecies::Ghost, false) => "(~oo~)",
        (super::model::BuddySpecies::Ghost, true) => "(~^^~)",
        (super::model::BuddySpecies::Robot, false) => "[o_o]",
        (super::model::BuddySpecies::Robot, true) => "[^_^]",
    }
}

fn rarity_style(bones: &BuddyBones) -> Style {
    let base = match bones.rarity {
        super::model::BuddyRarity::Common => Style::default(),
        super::model::BuddyRarity::Uncommon => Style::default().green(),
        super::model::BuddyRarity::Rare => Style::default().cyan(),
        super::model::BuddyRarity::Epic => Style::default().magenta(),
        super::model::BuddyRarity::Legendary => Style::default().magenta().bold(),
    };
    if bones.shiny { base.bold() } else { base }
}

fn truncate_with_ellipsis(text: &str, width: u16) -> String {
    let target_width = usize::from(width);
    let (truncated, _, _) = take_prefix_by_width(text, target_width);
    if truncated == text || target_width <= 1 {
        return truncated;
    }

    let (shortened, _, _) = take_prefix_by_width(text, target_width - 1);
    format!("{shortened}…")
}
