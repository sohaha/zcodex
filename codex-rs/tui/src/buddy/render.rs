use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use textwrap::Options;

use crate::live_wrap::take_prefix_by_width;

use super::model::BuddyBones;
use super::model::BuddyEye;
use super::model::BuddyFrame;
use super::model::BuddyHat;
use super::model::BuddyRarity;
use super::model::BuddySpecies;
use super::model::BuddyState;

const MIN_RENDER_WIDTH: u16 = 12;
const FULL_LAYOUT_WIDTH: u16 = 58;
const MAX_BUBBLE_WIDTH: usize = 34;
const NARROW_QUIP_CAP: usize = 26;
const PET_HEARTS: [&str; 5] = [
    "   <3    <3   ",
    "  <3  <3   <3 ",
    " <3   <3  <3  ",
    "<3  <3     <3 ",
    ".   .   .    .",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyRenderMode {
    InlineBubble,
    NoBubble,
}

pub(crate) fn full_layout_width() -> u16 {
    FULL_LAYOUT_WIDTH
}

pub(crate) fn render_lines(
    bones: &BuddyBones,
    name: &str,
    state: &BuddyState,
    width: u16,
    mode: BuddyRenderMode,
) -> Vec<Line<'static>> {
    if width < MIN_RENDER_WIDTH {
        return Vec::new();
    }

    if width < FULL_LAYOUT_WIDTH {
        return vec![render_narrow_line(bones, name, state, width)];
    }

    render_wide_lines(bones, name, state, width, mode)
}

fn render_wide_lines(
    bones: &BuddyBones,
    name: &str,
    state: &BuddyState,
    width: u16,
    mode: BuddyRenderMode,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if mode == BuddyRenderMode::InlineBubble {
        lines.extend(render_bubble_lines(bones, state, width));
    }
    if let Some(frame) = state.pet_burst_frame() {
        lines.push(Line::from(vec![
            "  ".into(),
            PET_HEARTS[frame].red().bold(),
        ]));
    }

    for sprite_line in sprite_lines(bones, state.frame()) {
        lines.push(Line::from(vec![
            "  ".into(),
            Span::styled(sprite_line, rarity_style(bones)),
        ]));
    }

    lines.push(render_identity_line(bones, name, state));
    lines
}

fn render_narrow_line(
    bones: &BuddyBones,
    name: &str,
    state: &BuddyState,
    width: u16,
) -> Line<'static> {
    let label = if let Some(text) = state.active_reaction_text() {
        let quip = truncate_with_ellipsis(text, NARROW_QUIP_CAP as u16);
        format!("\"{quip}\"")
    } else {
        let shiny = if bones.shiny { " *" } else { "" };
        format!(
            "{} {}{} {}",
            name,
            bones.rarity.stars(),
            shiny,
            bones.species.label()
        )
    };
    let face = mini_face(bones.species, state.frame());
    let prefix = if state.pet_burst_frame().is_some() {
        "<3 "
    } else {
        ""
    };
    let plain = format!("{prefix}{face} {label}");
    let truncated = truncate_with_ellipsis(&plain, width);
    if truncated != plain {
        return Line::from(truncated);
    }

    let mut spans = Vec::new();
    if state.pet_burst_frame().is_some() {
        spans.push("<3 ".red().bold());
    }
    spans.push(Span::styled(
        face.to_string(),
        rarity_style(bones).add_modifier(ratatui::style::Modifier::BOLD),
    ));
    spans.push(" ".into());
    if let Some(text) = state.active_reaction_text() {
        spans.push(format!("\"{text}\"").italic());
    } else {
        spans.push(name.to_string().cyan().bold());
        spans.push(" ".into());
        spans.push(bones.rarity.stars_span());
        if bones.shiny {
            spans.push(" *".yellow().bold());
        }
    }
    Line::from(spans)
}

pub(crate) fn render_bubble_lines(
    _bones: &BuddyBones,
    state: &BuddyState,
    width: u16,
) -> Vec<Line<'static>> {
    let Some(text) = state.active_reaction_text() else {
        return Vec::new();
    };
    render_bubble(text, width, state.reaction_is_fading())
}

fn render_bubble(text: &str, width: u16, fading: bool) -> Vec<Line<'static>> {
    let bubble_width = usize::from(width.saturating_sub(8)).clamp(18, MAX_BUBBLE_WIDTH);
    let wrapped = textwrap::wrap(text, Options::new(bubble_width));
    let body_width = wrapped
        .iter()
        .map(|line| line.len())
        .max()
        .unwrap_or_default();
    let border = if fading {
        Style::default().dim()
    } else {
        Style::default().cyan()
    };
    let text_style = if fading {
        Style::default().dim().italic()
    } else {
        Style::default().italic()
    };

    let mut lines = Vec::with_capacity(wrapped.len() + 3);
    lines.push(Line::from(vec![
        "  ".into(),
        Span::styled(format!(".{}.", "-".repeat(body_width + 2)), border),
    ]));

    for line in wrapped {
        lines.push(Line::from(vec![
            "  ".into(),
            Span::styled("| ", border),
            Span::styled(format!("{line:<body_width$}"), text_style),
            Span::styled(" |", border),
        ]));
    }

    lines.push(Line::from(vec![
        "  ".into(),
        Span::styled(format!("'{}.", "-".repeat(body_width + 2)), border),
    ]));
    lines.push(Line::from(vec![
        "    ".into(),
        Span::styled("\\".to_string(), border),
    ]));
    lines
}

fn render_identity_line(bones: &BuddyBones, name: &str, state: &BuddyState) -> Line<'static> {
    let mood = match state.frame() {
        BuddyFrame::Blink => "眨眼",
        BuddyFrame::FidgetUp | BuddyFrame::FidgetDown => "坐立不安",
        BuddyFrame::ExcitedA | BuddyFrame::ExcitedB => "兴奋",
        BuddyFrame::Rest => {
            if state.active_reaction_text().is_some() {
                "健谈"
            } else {
                "安静"
            }
        }
    };

    let mut spans = vec![
        "  ".into(),
        name.to_string().cyan().bold(),
        " ".into(),
        bones.rarity.stars_span(),
        " ".into(),
        bones.rarity.styled_span(),
        " ".into(),
        bones.species.label().dim(),
        " · ".dim(),
        mood.dim(),
    ];
    if bones.shiny {
        spans.push(" · ".dim());
        spans.push("闪亮".yellow().bold());
    }
    Line::from(spans)
}

fn sprite_lines(bones: &BuddyBones, frame: BuddyFrame) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(hat) = hat_line(bones.hat, frame) {
        lines.push(hat.to_string());
    }
    lines.extend(match bones.species {
        BuddySpecies::Cat => cat_lines(bones.eye, frame),
        BuddySpecies::Fox => fox_lines(bones.eye, frame),
        BuddySpecies::Otter => otter_lines(bones.eye, frame),
        BuddySpecies::Rabbit => rabbit_lines(bones.eye, frame),
        BuddySpecies::Owl => owl_lines(bones.eye, frame),
        BuddySpecies::Dragon => dragon_lines(bones.eye, frame),
        BuddySpecies::Ghost => ghost_lines(bones.eye, frame),
        BuddySpecies::Robot => robot_lines(bones.eye, frame),
        BuddySpecies::Duck => duck_lines(bones.eye, frame),
        BuddySpecies::Blob => blob_lines(bones.eye, frame),
        BuddySpecies::Octopus => octopus_lines(bones.eye, frame),
        BuddySpecies::Penguin => penguin_lines(bones.eye, frame),
        BuddySpecies::Turtle => turtle_lines(bones.eye, frame),
        BuddySpecies::Axolotl => axolotl_lines(bones.eye, frame),
        BuddySpecies::Capybara => capybara_lines(bones.eye, frame),
        BuddySpecies::Cactus => cactus_lines(bones.eye, frame),
        BuddySpecies::Mushroom => mushroom_lines(bones.eye, frame),
        BuddySpecies::Chonk => chonk_lines(bones.eye, frame),
    });
    lines
}

fn hat_line(hat: BuddyHat, frame: BuddyFrame) -> Option<&'static str> {
    let lively = matches!(frame, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB);
    match (hat, lively) {
        (BuddyHat::None, _) => None,
        (BuddyHat::Crown, false) => Some("   _/\\_   "),
        (BuddyHat::Crown, true) => Some("  _/\\/\\_  "),
        (BuddyHat::TopHat, false) => Some("   _____   "),
        (BuddyHat::TopHat, true) => Some("  ._____.  "),
        (BuddyHat::Halo, false) => Some("   .---.   "),
        (BuddyHat::Halo, true) => Some("   .-o-.   "),
        (BuddyHat::Wizard, false) => Some("    /\\\\    "),
        (BuddyHat::Wizard, true) => Some("    /\\/    "),
        (BuddyHat::Beanie, false) => Some("   _____   "),
        (BuddyHat::Beanie, true) => Some("   _===_   "),
        (BuddyHat::Propeller, false) => Some("  --(*)--  "),
        (BuddyHat::Propeller, true) => Some("  ==(*)==  "),
        (BuddyHat::TinyDuck, false) => Some("   ,>    "),
        (BuddyHat::TinyDuck, true) => Some("   ,>    "),
    }
}

fn cat_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let mouth = mouth(frame, "^", "w");
    [
        apply_offset("  /\\_/\\\\  ".to_string(), frame),
        apply_offset(format!(" ( {eye}{eye} ) "), frame),
        apply_offset(format!("  > {mouth} <  "), frame),
    ]
}

fn fox_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let mouth = mouth(frame, "v", "w");
    [
        apply_offset(" /\\   /\\\\ ".to_string(), frame),
        apply_offset(format!("( {eye} {mouth} {eye} )"), frame),
        apply_offset(" \\\\_---_//".to_string(), frame),
    ]
}

fn otter_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let mouth = mouth(frame, "_", "u");
    [
        apply_offset("  .-\"\"-.  ".to_string(), frame),
        apply_offset(format!(" / {eye}  {eye} \\\\"), frame),
        apply_offset(format!(" \\\\_={mouth}==_/"), frame),
    ]
}

fn rabbit_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let mouth = mouth(frame, "^", "w");
    [
        apply_offset("  (\\ /)   ".to_string(), frame),
        apply_offset(format!(" ( {eye} {eye} ) "), frame),
        apply_offset(format!(" /  {mouth}  \\\\ "), frame),
    ]
}

fn owl_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let brow = match frame {
        BuddyFrame::ExcitedA | BuddyFrame::ExcitedB => "^",
        _ => "_",
    };
    [
        apply_offset("  ,_,     ".to_string(), frame),
        apply_offset(format!(" ( {eye}{brow}{eye} ) "), frame),
        apply_offset(" /)___(\\\\ ".to_string(), frame),
    ]
}

fn dragon_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let mouth = mouth(frame, "~", "w");
    [
        apply_offset("  /\\_/\\\\  ".to_string(), frame),
        apply_offset(format!(" ( {eye} {mouth} {eye} )"), frame),
        apply_offset("  \\\\_v_// ".to_string(), frame),
    ]
}

fn ghost_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let fringe = match frame {
        BuddyFrame::ExcitedA | BuddyFrame::ExcitedB => "~^~~~",
        BuddyFrame::FidgetDown => "~~~~~",
        _ => " ~~~ ",
    };
    [
        apply_offset("  .---.   ".to_string(), frame),
        apply_offset(format!(" ( {eye} {eye} ) "), frame),
        apply_offset(format!(" /{fringe}\\\\ "), frame),
    ]
}

fn robot_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let mouth = match frame {
        BuddyFrame::ExcitedA | BuddyFrame::ExcitedB => "=",
        BuddyFrame::Blink => "-",
        _ => "_",
    };
    [
        apply_offset("  [---]   ".to_string(), frame),
        apply_offset(format!("  | {eye}{mouth}{eye} | "), frame),
        apply_offset("  /|___|\\\\ ".to_string(), frame),
    ]
}

fn duck_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset("  __     ".to_string(), frame),
        apply_offset(format!(" ( {eye}> )  "), frame),
        apply_offset("  `--'   ".to_string(), frame),
    ]
}

fn blob_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset("  .--.   ".to_string(), frame),
        apply_offset(format!(" ( {eye}{eye} )  "), frame),
        apply_offset("  `--'   ".to_string(), frame),
    ]
}

fn octopus_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset("  .--.   ".to_string(), frame),
        apply_offset(format!(" ( {eye}{eye} )  "), frame),
        apply_offset(" /\\/\\/\\  ".to_string(), frame),
    ]
}

fn penguin_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset("  .--.   ".to_string(), frame),
        apply_offset(format!(" ( {eye}>{eye} ) "), frame),
        apply_offset(" /(   )\\ ".to_string(), frame),
    ]
}

fn turtle_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset("  _--_   ".to_string(), frame),
        apply_offset(format!(" ( {eye}_{eye} ) "), frame),
        apply_offset(" /____\\ ".to_string(), frame),
    ]
}

fn axolotl_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset(" }~   ~{ ".to_string(), frame),
        apply_offset(format!(" ( {eye}..{eye} )"), frame),
        apply_offset("  \\__/   ".to_string(), frame),
    ]
}

fn capybara_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset("  n___n  ".to_string(), frame),
        apply_offset(format!(" ( {eye}  {eye} )"), frame),
        apply_offset(" ( 00 ) ".to_string(), frame),
    ]
}

fn cactus_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset("  _| |_  ".to_string(), frame),
        apply_offset(format!(" | {eye} {eye} | "), frame),
        apply_offset("  |_|_|  ".to_string(), frame),
    ]
}

fn mushroom_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    [
        apply_offset(" .-o-.   ".to_string(), frame),
        apply_offset(" (____)  ".to_string(), frame),
        apply_offset(format!("  |{eye}{eye}|   "), frame),
    ]
}

fn chonk_lines(eye: BuddyEye, frame: BuddyFrame) -> [String; 3] {
    let eye = eye_glyph(eye, frame);
    let mouth = mouth(frame, ".", "o");
    [
        apply_offset(" /\\___/\\ ".to_string(), frame),
        apply_offset(format!("( {eye}   {eye} )"), frame),
        apply_offset(format!(" (  {mouth}  ) "), frame),
    ]
}

fn mini_face(species: BuddySpecies, frame: BuddyFrame) -> &'static str {
    match (species, frame) {
        (BuddySpecies::Cat, BuddyFrame::Blink) => "(=-.-=)",
        (BuddySpecies::Cat, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(=^w^=)",
        (BuddySpecies::Cat, _) => "(=^.^=)",
        (BuddySpecies::Fox, BuddyFrame::Blink) => "(/\\-.-/\\\\)",
        (BuddySpecies::Fox, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(/\\^w^/\\\\)",
        (BuddySpecies::Fox, _) => "(/\\^.^/\\\\)",
        (BuddySpecies::Otter, BuddyFrame::Blink) => "(-3-)",
        (BuddySpecies::Otter, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(o^^o)",
        (BuddySpecies::Otter, _) => "(o3o)",
        (BuddySpecies::Rabbit, BuddyFrame::Blink) => "(\\\\-.-//)",
        (BuddySpecies::Rabbit, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(\\\\^w^//)",
        (BuddySpecies::Rabbit, _) => "(\\\\_//)",
        (BuddySpecies::Owl, BuddyFrame::Blink) => "(-v-)",
        (BuddySpecies::Owl, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(OwO)",
        (BuddySpecies::Owl, _) => "(OvO)",
        (BuddySpecies::Dragon, BuddyFrame::Blink) => "<:-.-:>",
        (BuddySpecies::Dragon, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "<:^w^:>",
        (BuddySpecies::Dragon, _) => "<:==:>",
        (BuddySpecies::Ghost, BuddyFrame::Blink) => "(~-~-)",
        (BuddySpecies::Ghost, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(~^^~)",
        (BuddySpecies::Ghost, _) => "(~oo~)",
        (BuddySpecies::Robot, BuddyFrame::Blink) => "[-_-]",
        (BuddySpecies::Robot, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "[^=^]",
        (BuddySpecies::Robot, _) => "[o_o]",
        (BuddySpecies::Duck, BuddyFrame::Blink) => "(->)",
        (BuddySpecies::Duck, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(^>)",
        (BuddySpecies::Duck, _) => "(o>)",
        (BuddySpecies::Blob, BuddyFrame::Blink) => "(-_-)",
        (BuddySpecies::Blob, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(^_^)",
        (BuddySpecies::Blob, _) => "(o_o)",
        (BuddySpecies::Octopus, BuddyFrame::Blink) => "~(- -)~",
        (BuddySpecies::Octopus, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "~(^_^)~",
        (BuddySpecies::Octopus, _) => "~(o_o)~",
        (BuddySpecies::Penguin, BuddyFrame::Blink) => "(- > -)",
        (BuddySpecies::Penguin, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(^>^)",
        (BuddySpecies::Penguin, _) => "(o>o)",
        (BuddySpecies::Turtle, BuddyFrame::Blink) => "[-_-]",
        (BuddySpecies::Turtle, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "[^_^]",
        (BuddySpecies::Turtle, _) => "[o_o]",
        (BuddySpecies::Axolotl, BuddyFrame::Blink) => "} -.- {",
        (BuddySpecies::Axolotl, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "}^.^{",
        (BuddySpecies::Axolotl, _) => "}o.o{",
        (BuddySpecies::Capybara, BuddyFrame::Blink) => "( - - )",
        (BuddySpecies::Capybara, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(^ ^)",
        (BuddySpecies::Capybara, _) => "(o o)",
        (BuddySpecies::Cactus, BuddyFrame::Blink) => "|- -|",
        (BuddySpecies::Cactus, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "|^ ^|",
        (BuddySpecies::Cactus, _) => "|o o|",
        (BuddySpecies::Mushroom, BuddyFrame::Blink) => "|- -|",
        (BuddySpecies::Mushroom, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "|^ ^|",
        (BuddySpecies::Mushroom, _) => "|o o|",
        (BuddySpecies::Chonk, BuddyFrame::Blink) => "(-.-)",
        (BuddySpecies::Chonk, BuddyFrame::ExcitedA | BuddyFrame::ExcitedB) => "(^.^)",
        (BuddySpecies::Chonk, _) => "(o.o)",
    }
}

fn eye_glyph(eye: BuddyEye, frame: BuddyFrame) -> &'static str {
    match frame {
        BuddyFrame::Blink => "-",
        BuddyFrame::ExcitedA | BuddyFrame::ExcitedB => eye.glyph(true),
        BuddyFrame::FidgetUp => "^",
        BuddyFrame::FidgetDown => "~",
        BuddyFrame::Rest => eye.glyph(false),
    }
}

fn mouth(frame: BuddyFrame, calm: &'static str, excited: &'static str) -> &'static str {
    match frame {
        BuddyFrame::Blink => calm,
        BuddyFrame::ExcitedA | BuddyFrame::ExcitedB => excited,
        BuddyFrame::FidgetUp | BuddyFrame::FidgetDown | BuddyFrame::Rest => calm,
    }
}

fn apply_offset(mut line: String, frame: BuddyFrame) -> String {
    match frame {
        BuddyFrame::FidgetUp => {
            if !line.is_empty() {
                line.remove(0);
                line.push(' ');
            }
            line
        }
        BuddyFrame::FidgetDown => {
            if !line.is_empty() {
                line.pop();
                line.insert(0, ' ');
            }
            line
        }
        BuddyFrame::Rest | BuddyFrame::Blink | BuddyFrame::ExcitedA | BuddyFrame::ExcitedB => line,
    }
}

fn rarity_style(bones: &BuddyBones) -> Style {
    let base = match bones.rarity {
        BuddyRarity::Common => Style::default(),
        BuddyRarity::Uncommon => Style::default().green(),
        BuddyRarity::Rare => Style::default().cyan(),
        BuddyRarity::Epic => Style::default().magenta(),
        BuddyRarity::Legendary => Style::default().yellow().bold(),
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
