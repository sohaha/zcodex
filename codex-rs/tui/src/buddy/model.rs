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

const IDLE_SEQUENCE: [BuddyFrame; 12] = [
    BuddyFrame::FidgetUp,
    BuddyFrame::Rest,
    BuddyFrame::FidgetDown,
    BuddyFrame::Rest,
    BuddyFrame::Blink,
    BuddyFrame::Rest,
    BuddyFrame::FidgetUp,
    BuddyFrame::Rest,
    BuddyFrame::FidgetDown,
    BuddyFrame::Rest,
    BuddyFrame::Blink,
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
            Self::Cat => "猫",
            Self::Fox => "狐狸",
            Self::Otter => "水獭",
            Self::Rabbit => "兔子",
            Self::Owl => "猫头鹰",
            Self::Dragon => "龙",
            Self::Ghost => "幽灵",
            Self::Robot => "机器人",
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
                "踏进底栏，像本来就该在那儿。",
                "带着一副主宰终端的猫式自信出现。",
            ],
            Self::Fox => &[
                "斜瞥一眼，尾巴一甩就到了。",
                "从回滚区走出来，满意得有点可疑。",
            ],
            Self::Otter => &[
                "像在河石上滑行一样滑进底栏。",
                "冒出来，嘴里叼着一块过分光滑的石子。",
            ],
            Self::Rabbit => &[
                "蹦进视野，只停一会儿卖个萌。",
                "落在底栏，带着一点小小的胜利跳。",
            ],
            Self::Owl => &[
                "安坐下来，那目光带着奇妙的管理气场。",
                "滑翔到位，立刻露出不太买账的表情。",
            ],
            Self::Dragon => &[
                "从一簇火星中伸展开，把底栏当成宝库。",
                "伴着一小撮戏剧性的烟雾现身。",
            ],
            Self::Ghost => &["礼貌地从底栏飘上来。", "安静现身，像一直在这块面板里徘徊。"],
            Self::Robot => &["清脆启动，动作干净利落。", "以令人舒适的机械精准折叠到位。"],
        }
    }

    pub(crate) fn return_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &[
                "假装从未离开，继续监督你。",
                "回来前就判断你的工作需要管一管。",
            ],
            Self::Fox => &[
                "再次出现，像早就预料到了这一刻。",
                "按它认为你该得到的戏剧量回来了。",
            ],
            Self::Otter => &[
                "晃晃悠悠回到视野里，依然轻快。",
                "回来就把底栏的气氛提亮了。",
            ],
            Self::Rabbit => &["趁安静变尴尬前蹦回来了。", "耳朵竖起，注意力全开地回来了。"],
            Self::Owl => &[
                "回到栖木上，评判意味溢于言表。",
                "再度出现，像夜间代码审查开始了。",
            ],
            Self::Dragon => &[
                "带着低沉的轰鸣回归，自信得不容置疑。",
                "像被未尽的野心召唤般再次舒展开。",
            ],
            Self::Ghost => &["飘回来，一字节也没惊动。", "轻轻回来，但一点也不含蓄。"],
            Self::Robot => &["精准地归位。", "在一次看似成功的待机循环后重新出现。"],
        }
    }

    pub(crate) fn pet_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &[
                "立刻带着威严贴上来求抚摸。",
                "呼噜声大到让输入框都在震。",
                "半眯着眼，接受你的供奉。",
            ],
            Self::Fox => &[
                "先高冷一秒，然后瞬间融化。",
                "甩甩尾巴，批准你继续。",
                "对效果很满意，表情很得意。",
            ],
            Self::Otter => &[
                "在干地上也抖出一点水花。",
                "回赠你一颗光滑得过分的石子。",
                "在底栏里幸福地翻了个身。",
            ],
            Self::Rabbit => &[
                "小小胜利一跳，靠得更近。",
                "鼻子一动，明显是好评。",
                "以一种可疑的快乐方式安静下来。",
            ],
            Self::Owl => &[
                "郑重地眨了下眼，却莫名亲切。",
                "抖抖羽毛，严厉程度稍减。",
                "像尊贵的夜行君主一样接受抚摸。",
            ],
            Self::Dragon => &[
                "吐出一缕满意的火星。",
                "带着炉火般的得意贴了过来。",
                "短暂发光，戏剧化地骄傲了一下。",
            ],
            Self::Ghost => &[
                "开心地闪烁，但依旧摸不着。",
                "开心地转了一圈小旋涡。",
                "变得更透明，显然很满意。",
            ],
            Self::Robot => &[
                "发出满意的咔哒声，准备再来。",
                "把这次互动记录为最佳士气维护。",
                "小小庆祝式伺服舞蹈走一套。",
            ],
        }
    }

    pub(crate) fn teaser_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &[
                "在附近趴成一团。试试 /buddy pet。",
                "耳朵一动，像在等你打招呼。",
            ],
            Self::Fox => &["正用可疑的魅力盯着底栏。", "歪着头，好像已经知道你下一步。"],
            Self::Otter => &[
                "冒出水面，叼着石子微微一笑。",
                "准备用好心情换一次 /buddy pet。",
            ],
            Self::Rabbit => &["轻轻一跳，等你注意。", "就在这儿，警觉又特别好摸。"],
            Self::Owl => &["在输入框旁落了座。", "眨一下眼，像安静的代码审查邀请。"],
            Self::Dragon => &[
                "像一簇温暖的火星盘在底栏。",
                "喷出一小撮带着野心味道的火星。",
            ],
            Self::Ghost => &["礼貌地漂浮在你的提示旁。", "用一只半透明的爪子挥了挥。"],
            Self::Robot => &["进入待机并请求一次抚摸。", "报告士气系统在线且很可爱。"],
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
            Self::Dot => "点点",
            Self::Spark => "星光",
            Self::Cross => "叉叉",
            Self::Wide => "圆眼",
            Self::Sleepy => "困倦",
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
            Self::None => "无",
            Self::Crown => "王冠",
            Self::TopHat => "高礼帽",
            Self::Halo => "光环",
            Self::Wizard => "巫师帽",
            Self::Beanie => "毛线帽",
            Self::Propeller => "螺旋帽",
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
            Self::Debugging => "调试",
            Self::Patience => "耐心",
            Self::Chaos => "混沌",
            Self::Wisdom => "智慧",
            Self::Snark => "吐槽",
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
            Self::Common => "常见",
            Self::Uncommon => "少见",
            Self::Rare => "稀有",
            Self::Epic => "史诗",
            Self::Legendary => "传奇",
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
