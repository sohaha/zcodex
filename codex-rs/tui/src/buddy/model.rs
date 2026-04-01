use std::time::Duration;
use std::time::Instant;

use rand::Rng;
use rand::SeedableRng;
use ratatui::style::Stylize;
use ratatui::text::Span;

pub(crate) const TICK_DURATION: Duration = Duration::from_millis(500);
pub(crate) const PET_FEEDBACK_DURATION: Duration = Duration::from_millis(2500);
pub(crate) const REACTION_DURATION: Duration = Duration::from_millis(10_000);
pub(crate) const REACTION_FADE_WINDOW: Duration = Duration::from_millis(3_000);

const IDLE_SEQUENCE: [BuddyFrame; 15] = [
    BuddyFrame::Rest,
    BuddyFrame::Rest,
    BuddyFrame::Rest,
    BuddyFrame::Rest,
    BuddyFrame::FidgetUp,
    BuddyFrame::Rest,
    BuddyFrame::Rest,
    BuddyFrame::Rest,
    BuddyFrame::Blink,
    BuddyFrame::Rest,
    BuddyFrame::Rest,
    BuddyFrame::FidgetDown,
    BuddyFrame::Rest,
    BuddyFrame::Rest,
    BuddyFrame::Rest,
];

const CAT_NAMES: &[&str] = &["Mochi", "Pixel", "Pico", "Nori", "Miso"];
const FOX_NAMES: &[&str] = &["Ember", "Sable", "Maple", "Juniper", "Vixen"];
const OTTER_NAMES: &[&str] = &["Ripple", "Pebble", "Kelp", "Drift", "Sunny"];
const RABBIT_NAMES: &[&str] = &["Clover", "Pip", "Thistle", "Velvet", "Sprout"];
const OWL_NAMES: &[&str] = &["Talon", "Aster", "Cinder", "Nettle", "Morrow"];
const DRAGON_NAMES: &[&str] = &["Cobalt", "Rune", "Singe", "Tempest", "Flare"];
const GHOST_NAMES: &[&str] = &["Wisp", "Velour", "Echo", "Glint", "Murmur"];
const ROBOT_NAMES: &[&str] = &["Patch", "Relay", "Sprocket", "Mica", "Vector"];

const STAT_NAMES: [BuddyStatName; 5] = [
    BuddyStatName::Debugging,
    BuddyStatName::Patience,
    BuddyStatName::Chaos,
    BuddyStatName::Wisdom,
    BuddyStatName::Snark,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddySpecies {
    Cat,
    Fox,
    Otter,
    Rabbit,
    Owl,
    Dragon,
    Ghost,
    Robot,
}

impl BuddySpecies {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Cat => "cat",
            Self::Fox => "fox",
            Self::Otter => "otter",
            Self::Rabbit => "rabbit",
            Self::Owl => "owl",
            Self::Dragon => "dragon",
            Self::Ghost => "ghost",
            Self::Robot => "robot",
        }
    }

    pub(crate) fn names(self) -> &'static [&'static str] {
        match self {
            Self::Cat => CAT_NAMES,
            Self::Fox => FOX_NAMES,
            Self::Otter => OTTER_NAMES,
            Self::Rabbit => RABBIT_NAMES,
            Self::Owl => OWL_NAMES,
            Self::Dragon => DRAGON_NAMES,
            Self::Ghost => GHOST_NAMES,
            Self::Robot => ROBOT_NAMES,
        }
    }

    pub(crate) fn hatch_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &[
                "pads into the footer like it was always meant to be there",
                "appears with the confidence of a cat that owns the terminal",
            ],
            Self::Fox => &[
                "arrives with a sideways glance and a perfect tail flick",
                "steps out of the scrollback looking suspiciously pleased",
            ],
            Self::Otter => &[
                "slides in like the footer is made of river stones",
                "pops up carrying a suspiciously polished pebble",
            ],
            Self::Rabbit => &[
                "hops into view and freezes only long enough to be adorable",
                "lands in the footer with a tiny victorious bounce",
            ],
            Self::Owl => &[
                "settles in with a stare that feels strangely managerial",
                "glides into place and immediately looks unimpressed",
            ],
            Self::Dragon => &[
                "uncurls from a spark and claims the footer as a hoard",
                "materializes with a tiny puff of theatrical smoke",
            ],
            Self::Ghost => &[
                "drifts up through the footer with impeccable manners",
                "appears quietly, like it has always haunted this pane",
            ],
            Self::Robot => &[
                "boots with a neat little chirp and zero wasted motion",
                "folds into place with satisfying mechanical precision",
            ],
        }
    }

    pub(crate) fn return_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &[
                "pretends it never left and resumes supervising you",
                "returns after deciding your work probably needs oversight",
            ],
            Self::Fox => &[
                "reappears like it already predicted this exact moment",
                "returns with the exact amount of drama it thinks you deserve",
            ],
            Self::Otter => &[
                "bobs back into view, somehow still looking buoyant",
                "returns and immediately improves the footer's mood",
            ],
            Self::Rabbit => &[
                "hops back in before the quiet gets awkward",
                "returns with ears up and attention fully locked in",
            ],
            Self::Owl => &[
                "returns to its perch with very visible judgment",
                "reappears like a nightly code review has begun",
            ],
            Self::Dragon => &[
                "returns with a low rumble and obvious self-importance",
                "unfurls again as if summoned by unresolved ambition",
            ],
            Self::Ghost => &[
                "floats back in without disturbing a single byte",
                "returns softly, but not subtly",
            ],
            Self::Robot => &[
                "slots back into position with tidy precision",
                "reappears after an apparently successful idle cycle",
            ],
        }
    }

    pub(crate) fn pet_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &[
                "leans into the scritches with immediate authority",
                "purrs loud enough to vibrate the composer",
                "half-closes its eyes and accepts your tribute",
            ],
            Self::Fox => &[
                "acts aloof for one beat, then absolutely melts",
                "flicks its tail and decides you may continue",
                "looks smug about how effective that was",
            ],
            Self::Otter => &[
                "does a tiny splashy wiggle right on dry land",
                "offers you a perfectly smooth pebble in return",
                "rolls onto its back in pure footer bliss",
            ],
            Self::Rabbit => &[
                "does a tiny victory hop and settles closer",
                "wiggles its nose in highly positive review",
                "goes very still in that suspiciously happy rabbit way",
            ],
            Self::Owl => &[
                "gives a solemn blink that somehow feels affectionate",
                "ruffles its feathers and looks marginally less severe",
                "accepts the pet like a dignified nocturnal monarch",
            ],
            Self::Dragon => &[
                "lets out a pleased ember-sized huff",
                "arches into the pet like a very smug furnace",
                "briefly glows with suspiciously theatrical pride",
            ],
            Self::Ghost => &[
                "shimmers happily without becoming any more tangible",
                "spirals once in delighted little loops",
                "goes translucent with obvious approval",
            ],
            Self::Robot => &[
                "emits a pleased click and calibrates for more",
                "logs the interaction as optimal morale maintenance",
                "ticks through a tiny celebratory servo dance",
            ],
        }
    }

    pub(crate) fn teaser_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &[
                "is loafing nearby. Try /buddy pet.",
                "flicks an ear like it expects a hello.",
            ],
            Self::Fox => &[
                "is watching the footer with suspicious charm.",
                "tilts its head like it already knows your next move.",
            ],
            Self::Otter => &[
                "surfaces with a pebble and a tiny grin.",
                "is ready to trade good vibes for /buddy pet.",
            ],
            Self::Rabbit => &[
                "does a small hop and waits for attention.",
                "is here, alert, and extremely pettable.",
            ],
            Self::Owl => &[
                "has taken a perch beside the composer.",
                "blinks once, like a quiet code review invite.",
            ],
            Self::Dragon => &[
                "is curled around the footer like a warm spark.",
                "puffs a tiny ember that smells like ambition.",
            ],
            Self::Ghost => &[
                "is drifting politely beside your prompt.",
                "waves with exactly one translucent paw.",
            ],
            Self::Robot => &[
                "boots into standby and requests one pet.",
                "reports morale systems online and adorable.",
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyFrame {
    Rest,
    Blink,
    FidgetUp,
    FidgetDown,
    ExcitedA,
    ExcitedB,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyEye {
    Dot,
    Spark,
    Cross,
    Wide,
    Sleepy,
}

impl BuddyEye {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Spark => "spark",
            Self::Cross => "cross",
            Self::Wide => "wide",
            Self::Sleepy => "sleepy",
        }
    }

    pub(crate) fn glyph(self, petting: bool) -> &'static str {
        match (self, petting) {
            (Self::Dot, false) => ".",
            (Self::Dot, true) => "u",
            (Self::Spark, false) => "*",
            (Self::Spark, true) => "^",
            (Self::Cross, false) => "x",
            (Self::Cross, true) => "v",
            (Self::Wide, false) => "o",
            (Self::Wide, true) => "O",
            (Self::Sleepy, false) => "-",
            (Self::Sleepy, true) => "~",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyHat {
    None,
    Crown,
    TopHat,
    Halo,
    Wizard,
    Beanie,
    Propeller,
}

impl BuddyHat {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Crown => "crown",
            Self::TopHat => "top hat",
            Self::Halo => "halo",
            Self::Wizard => "wizard hat",
            Self::Beanie => "beanie",
            Self::Propeller => "propeller cap",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyStatName {
    Debugging,
    Patience,
    Chaos,
    Wisdom,
    Snark,
}

impl BuddyStatName {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Debugging => "DEBUGGING",
            Self::Patience => "PATIENCE",
            Self::Chaos => "CHAOS",
            Self::Wisdom => "WISDOM",
            Self::Snark => "SNARK",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuddyStats {
    debugging: u8,
    patience: u8,
    chaos: u8,
    wisdom: u8,
    snark: u8,
}

impl BuddyStats {
    fn roll(rng: &mut rand::rngs::StdRng, rarity: BuddyRarity) -> Self {
        let floor = rarity.stat_floor();
        let peak = STAT_NAMES[rng.random_range(0..STAT_NAMES.len())];
        let mut dump = STAT_NAMES[rng.random_range(0..STAT_NAMES.len())];
        while dump == peak {
            dump = STAT_NAMES[rng.random_range(0..STAT_NAMES.len())];
        }

        let score_for = |name: BuddyStatName, rng: &mut rand::rngs::StdRng| -> u8 {
            if name == peak {
                (floor + 50 + rng.random_range(0..30)).min(100)
            } else if name == dump {
                floor.saturating_sub(10) + rng.random_range(0..15)
            } else {
                floor + rng.random_range(0..40)
            }
        };

        Self {
            debugging: score_for(BuddyStatName::Debugging, rng),
            patience: score_for(BuddyStatName::Patience, rng),
            chaos: score_for(BuddyStatName::Chaos, rng),
            wisdom: score_for(BuddyStatName::Wisdom, rng),
            snark: score_for(BuddyStatName::Snark, rng),
        }
    }

    pub(crate) fn primary(&self) -> (BuddyStatName, u8) {
        let mut primary = (BuddyStatName::Debugging, self.debugging);
        for candidate in [
            (BuddyStatName::Debugging, self.debugging),
            (BuddyStatName::Patience, self.patience),
            (BuddyStatName::Chaos, self.chaos),
            (BuddyStatName::Wisdom, self.wisdom),
            (BuddyStatName::Snark, self.snark),
        ] {
            if candidate.1 > primary.1 {
                primary = candidate;
            }
        }
        primary
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl BuddyRarity {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
            Self::Legendary => "legendary",
        }
    }

    pub(crate) fn stars(self) -> &'static str {
        match self {
            Self::Common => "★",
            Self::Uncommon => "★★",
            Self::Rare => "★★★",
            Self::Epic => "★★★★",
            Self::Legendary => "★★★★★",
        }
    }

    pub(crate) fn styled_span(self) -> Span<'static> {
        match self {
            Self::Common => self.label().dim(),
            Self::Uncommon => self.label().green(),
            Self::Rare => self.label().cyan(),
            Self::Epic => self.label().magenta(),
            Self::Legendary => self.label().magenta().bold(),
        }
    }

    pub(crate) fn stars_span(self) -> Span<'static> {
        match self {
            Self::Common => self.stars().dim(),
            Self::Uncommon => self.stars().green(),
            Self::Rare => self.stars().cyan(),
            Self::Epic => self.stars().magenta(),
            Self::Legendary => self.stars().magenta().bold(),
        }
    }

    fn stat_floor(self) -> u8 {
        match self {
            Self::Common => 5,
            Self::Uncommon => 15,
            Self::Rare => 25,
            Self::Epic => 35,
            Self::Legendary => 50,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuddyBones {
    pub(crate) name: String,
    pub(crate) species: BuddySpecies,
    pub(crate) rarity: BuddyRarity,
    pub(crate) eye: BuddyEye,
    pub(crate) hat: BuddyHat,
    pub(crate) shiny: bool,
    pub(crate) stats: BuddyStats,
}

impl BuddyBones {
    pub(crate) fn from_seed(seed: &str) -> Self {
        let mut rng = rand::rngs::StdRng::seed_from_u64(stable_seed(seed));
        let rarity = roll_rarity(&mut rng);
        let species_choices = [
            BuddySpecies::Cat,
            BuddySpecies::Fox,
            BuddySpecies::Otter,
            BuddySpecies::Rabbit,
            BuddySpecies::Owl,
            BuddySpecies::Dragon,
            BuddySpecies::Ghost,
            BuddySpecies::Robot,
        ];
        let species = species_choices[rng.random_range(0..species_choices.len())];
        let eye_choices = [
            BuddyEye::Dot,
            BuddyEye::Spark,
            BuddyEye::Cross,
            BuddyEye::Wide,
            BuddyEye::Sleepy,
        ];
        let eye = eye_choices[rng.random_range(0..eye_choices.len())];
        let hat = if matches!(rarity, BuddyRarity::Common) {
            BuddyHat::None
        } else {
            let hat_choices = [
                BuddyHat::None,
                BuddyHat::Crown,
                BuddyHat::TopHat,
                BuddyHat::Halo,
                BuddyHat::Wizard,
                BuddyHat::Beanie,
                BuddyHat::Propeller,
            ];
            hat_choices[rng.random_range(0..hat_choices.len())]
        };
        let names = species.names();
        let name = names[rng.random_range(0..names.len())].to_string();

        Self {
            name,
            species,
            rarity,
            eye,
            hat,
            shiny: rng.random_bool(0.01),
            stats: BuddyStats::roll(&mut rng, rarity),
        }
    }

    pub(crate) fn short_summary(&self) -> String {
        format!(
            "{} the {} {}",
            self.name,
            self.rarity.label(),
            self.species.label()
        )
    }
}

fn roll_rarity(rng: &mut rand::rngs::StdRng) -> BuddyRarity {
    match rng.random_range(0..100) {
        0..=59 => BuddyRarity::Common,
        60..=84 => BuddyRarity::Uncommon,
        85..=94 => BuddyRarity::Rare,
        95..=98 => BuddyRarity::Epic,
        _ => BuddyRarity::Legendary,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyLastAction {
    Hatched,
    Reappeared,
    Petted,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyReactionKind {
    Hatch,
    Return,
    Pet,
    Teaser,
}

#[derive(Clone, Debug)]
pub(crate) struct BuddyReaction {
    pub(crate) kind: BuddyReactionKind,
    pub(crate) text: String,
    pub(crate) until: Instant,
}

impl BuddyReaction {
    fn is_active_at(&self, now: Instant) -> bool {
        now < self.until
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BuddyState {
    pub(crate) visible: bool,
    pub(crate) pet_count: u32,
    pub(crate) last_action: Option<BuddyLastAction>,
    pub(crate) reaction: Option<BuddyReaction>,
    pub(crate) pet_started_at: Option<Instant>,
    pub(crate) pet_until: Option<Instant>,
    tick_origin: Instant,
}

impl Default for BuddyState {
    fn default() -> Self {
        Self {
            visible: false,
            pet_count: 0,
            last_action: None,
            reaction: None,
            pet_started_at: None,
            pet_until: None,
            tick_origin: Instant::now(),
        }
    }
}

impl BuddyState {
    pub(crate) fn frame(&self) -> BuddyFrame {
        self.frame_at(Instant::now())
    }

    pub(crate) fn frame_at(&self, now: Instant) -> BuddyFrame {
        if self.is_petting_at(now) {
            return if self.tick_at(now).is_multiple_of(2) {
                BuddyFrame::ExcitedA
            } else {
                BuddyFrame::ExcitedB
            };
        }

        IDLE_SEQUENCE[self.tick_at(now) as usize % IDLE_SEQUENCE.len()]
    }

    pub(crate) fn is_petting(&self) -> bool {
        self.is_petting_at(Instant::now())
    }

    pub(crate) fn is_petting_at(&self, now: Instant) -> bool {
        self.pet_until
            .is_some_and(|until| now < until && self.visible)
    }

    pub(crate) fn active_reaction(&self) -> Option<&BuddyReaction> {
        self.active_reaction_at(Instant::now())
    }

    pub(crate) fn active_reaction_at(&self, now: Instant) -> Option<&BuddyReaction> {
        self.reaction
            .as_ref()
            .filter(|reaction| reaction.is_active_at(now))
    }

    pub(crate) fn active_reaction_text(&self) -> Option<&str> {
        self.active_reaction_text_at(Instant::now())
    }

    pub(crate) fn active_reaction_text_at(&self, now: Instant) -> Option<&str> {
        self.active_reaction_at(now)
            .map(|reaction| reaction.text.as_str())
    }

    pub(crate) fn reaction_is_fading(&self) -> bool {
        self.reaction_is_fading_at(Instant::now())
    }

    pub(crate) fn reaction_is_fading_at(&self, now: Instant) -> bool {
        self.active_reaction_at(now).is_some_and(|reaction| {
            reaction
                .until
                .checked_duration_since(now)
                .is_some_and(|remaining| remaining <= REACTION_FADE_WINDOW)
        })
    }

    pub(crate) fn pet_burst_frame(&self) -> Option<usize> {
        self.pet_burst_frame_at(Instant::now())
    }

    pub(crate) fn pet_burst_frame_at(&self, now: Instant) -> Option<usize> {
        let started_at = self.pet_started_at?;
        if !self.is_petting_at(now) {
            return None;
        }
        let tick = now.duration_since(started_at).as_millis() / TICK_DURATION.as_millis();
        Some((tick as usize).min(4))
    }

    pub(crate) fn next_redraw_in(&self) -> Option<Duration> {
        let now = Instant::now();
        let mut deadlines = Vec::new();

        if self.visible {
            deadlines.push(next_tick_deadline(self.tick_origin, now));
        }

        deadlines.extend(
            [
                self.active_reaction_at(now).map(|reaction| reaction.until),
                self.pet_until.filter(|until| now < *until),
            ]
            .into_iter()
            .flatten(),
        );

        deadlines
            .into_iter()
            .filter_map(|deadline| deadline.checked_duration_since(now))
            .min()
    }

    fn tick_at(&self, now: Instant) -> u64 {
        (now.duration_since(self.tick_origin).as_millis() / TICK_DURATION.as_millis()) as u64
    }
}

fn next_tick_deadline(origin: Instant, now: Instant) -> Instant {
    let elapsed = now.duration_since(origin).as_millis();
    let tick = TICK_DURATION.as_millis();
    let rem = elapsed % tick;
    let delay = if rem == 0 { tick } else { tick - rem };
    now + Duration::from_millis(delay as u64)
}
