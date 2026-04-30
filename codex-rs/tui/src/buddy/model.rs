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
pub(crate) const FULL_LAYOUT_INTRO_DURATION: Duration = Duration::from_millis(6_000);

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

const CAT_NAMES: &[&str] = &["年糕", "花卷", "小橘", "芝麻", "豆沙"];
const FOX_NAMES: &[&str] = &["小枫", "琥珀", "松果", "红豆", "阿赤"];
const OTTER_NAMES: &[&str] = &["小溪", "浪花", "圆圆", "海苔", "暖阳"];
const RABBIT_NAMES: &[&str] = &["三叶", "绒绒", "嫩芽", "棉花", "小跳"];
const OWL_NAMES: &[&str] = &["弯弯", "晓晓", "灰灰", "夜夜", "星星"];
const DRAGON_NAMES: &[&str] = &["小焰", "龙宝", "喷喷", "小雷", "飞飞"];
const GHOST_NAMES: &[&str] = &["飘飘", "烟烟", "幽幽", "萤萤", "念念"];
const ROBOT_NAMES: &[&str] = &["补丁", "哔哔", "铛铛", "小芯", "铮铮"];
const DUCK_NAMES: &[&str] = &["扁扁", "水洼", "羽笔", "扑腾", "小黄"];
const GOOSE_NAMES: &[&str] = &["嘎嘎", "饭团", "大白", "小灰", "胖胖"];
const BLOB_NAMES: &[&str] = &["滴溜", "呼噜", "融融", "咕噜", "软软"];
const OCTOPUS_NAMES: &[&str] = &["墨墨", "缠缠", "鼓鼓", "漩涡", "触触"];
const PENGUIN_NAMES: &[&str] = &["霜花", "凛凛", "滑翔", "摇摇", "冰柱"];
const TURTLE_NAMES: &[&str] = &["龟龟", "游游", "青青", "慢慢", "躲躲"];
const SNAIL_NAMES: &[&str] = &["壳壳", "露珠", "嘟嘟", "拖拖", "粘粘"];
const AXOLOTL_NAMES: &[&str] = &["阿索", "鳃花", "涟漪", "绽放", "尼莫"];
const CAPYBARA_NAMES: &[&str] = &["豚豚", "摩卡", "圣代", "暖暖", "萌萌"];
const CACTUS_NAMES: &[&str] = &["刺刺", "仙人", "艾草", "芦荟", "滚滚"];
const MUSHROOM_NAMES: &[&str] = &["小帽", "菇菇", "噗噗", "松露", "小伞"];
const CHONK_NAMES: &[&str] = &["胖墩", "饼干", "布丁", "云纹", "肉肉"];

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
    Duck,
    Goose,
    Blob,
    Octopus,
    Penguin,
    Turtle,
    Snail,
    Axolotl,
    Capybara,
    Cactus,
    Mushroom,
    Chonk,
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
            Self::Duck => "鸭子",
            Self::Goose => "鹅",
            Self::Blob => "史莱姆",
            Self::Octopus => "章鱼",
            Self::Penguin => "企鹅",
            Self::Turtle => "乌龟",
            Self::Snail => "蜗牛",
            Self::Axolotl => "美西螈",
            Self::Capybara => "水豚",
            Self::Cactus => "仙人掌",
            Self::Mushroom => "蘑菇",
            Self::Chonk => "团子",
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
            Self::Duck => DUCK_NAMES,
            Self::Goose => GOOSE_NAMES,
            Self::Blob => BLOB_NAMES,
            Self::Octopus => OCTOPUS_NAMES,
            Self::Penguin => PENGUIN_NAMES,
            Self::Turtle => TURTLE_NAMES,
            Self::Snail => SNAIL_NAMES,
            Self::Axolotl => AXOLOTL_NAMES,
            Self::Capybara => CAPYBARA_NAMES,
            Self::Cactus => CACTUS_NAMES,
            Self::Mushroom => MUSHROOM_NAMES,
            Self::Chonk => CHONK_NAMES,
        }
    }

    pub(crate) fn hatch_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &["轻巧落座。", "像在巡场一样现身。"],
            Self::Fox => &["尾巴一甩就到了。", "带着点得意出现。"],
            Self::Otter => &["顺着底栏滑进来。", "叼着石子冒了头。"],
            Self::Rabbit => &["轻轻蹦进视野。", "落地时还带点雀跃。"],
            Self::Owl => &["安静落座。", "像来巡夜一样现身。"],
            Self::Dragon => &["伴着一点火星现身。", "带着小小烟雾到了。"],
            Self::Ghost => &["礼貌地飘上来。", "安静地浮现。"],
            Self::Robot => &["清脆启动。", "利落归位。"],
            Self::Duck => &["摇摇摆摆走来。", "嘎一声报到。"],
            Self::Goose => &["昂着脖子巡了过来。", "像来接管底栏一样落位。"],
            Self::Blob => &["软软滚了过来。", "慢慢摊开坐好。"],
            Self::Octopus => &["触手先到了。", "像进了小潮池。"],
            Self::Penguin => &["带着凉意滑进来。", "拍着翅膀站好。"],
            Self::Turtle => &["慢悠悠挪到位。", "稳稳停好。"],
            Self::Snail => &["背着壳慢慢滑来。", "安静地把自己停在一边。"],
            Self::Axolotl => &["轻轻漂来。", "摆摆小鳃落座。"],
            Self::Capybara => &["悠然趴下。", "像来晒太阳一样。"],
            Self::Cactus => &["稳稳站好。", "一点也不慌。"],
            Self::Mushroom => &["像雨后一样冒出来。", "轻轻一抖就站好。"],
            Self::Chonk => &["一团快乐挤进来。", "软乎乎占了个座。"],
        }
    }

    pub(crate) fn return_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &["像没离开过。", "回来继续看着你。"],
            Self::Fox => &["得意地回来了。", "像早就料到你会叫它。"],
            Self::Otter => &["轻快地晃回来。", "回来就把气氛带亮了。"],
            Self::Rabbit => &["蹦回来了。", "耳朵一竖就到位。"],
            Self::Owl => &["回到栖木上。", "像审查又开始了。"],
            Self::Dragon => &["带着低鸣回归。", "又把气场铺开了。"],
            Self::Ghost => &["轻轻飘回来。", "一点声响都没有。"],
            Self::Robot => &["精准归位。", "待机后重新上线。"],
            Self::Duck => &["摇摆着回到视野。", "嘎一声催你继续。"],
            Self::Goose => &["昂首阔步地回来了。", "像准备继续维持秩序。"],
            Self::Blob => &["又软软摊开了。", "回到原位继续待着。"],
            Self::Octopus => &["触手绕了回来。", "稳稳占回位置。"],
            Self::Penguin => &["带着小凉意回来了。", "滑回位，站得很稳。"],
            Self::Turtle => &["慢慢回来。", "壳一落地就稳了。"],
            Self::Snail => &["慢悠悠滑回来了。", "把壳稳稳停在你身边。"],
            Self::Axolotl => &["轻轻漂回。", "小鳃一动就到你身边。"],
            Self::Capybara => &["淡定地回来了。", "又悠然趴好。"],
            Self::Cactus => &["稳稳站回原位。", "刺一点都没乱。"],
            Self::Mushroom => &["又冒出来了。", "蘑菇帽一晃就到位。"],
            Self::Chonk => &["滚回来坐好。", "软乎乎地回到旁边。"],
        }
    }

    pub(crate) fn pet_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &["呼噜得很响。", "半眯着眼贴近。", "满意地蹭了蹭。"],
            Self::Fox => &[
                "高冷只维持了一秒。",
                "尾巴一甩，心情很好。",
                "得意地靠近了点。",
            ],
            Self::Otter => &["抖出一点水花。", "递来一颗小石子。", "开心地翻了个身。"],
            Self::Rabbit => &["轻轻跳了一下。", "鼻子一动，明显满意。", "安静地靠近一点。"],
            Self::Owl => &[
                "郑重地眨了下眼。",
                "抖抖羽毛，神情柔和了。",
                "很克制地表示开心。",
            ],
            Self::Dragon => &["吐出一缕火星。", "带着得意靠近。", "短暂亮了一下。"],
            Self::Ghost => &["开心地闪了一下。", "转了个小旋涡。", "变得更透明了。"],
            Self::Robot => &[
                "发出满意的咔哒声。",
                "把这次互动记成优秀。",
                "跳了段小小伺服舞。",
            ],
            Self::Duck => &["嘎嘎回应。", "开心地抖抖羽毛。", "贴过来求更多。"],
            Self::Goose => &[
                "矜持地点了点头。",
                "满意地抖了抖翅膀。",
                "像夸你摸得很专业。",
            ],
            Self::Blob => &["开心地弹了一下。", "软乎乎贴过来。", "晃了晃像在道谢。"],
            Self::Octopus => &["触手轻轻缠了一下。", "把开心藏在触手里。", "点点头回应你。"],
            Self::Penguin => &["翅膀一摆，心情更好。", "微微摇晃了一下。", "贴过来蹭蹭。"],
            Self::Turtle => &["慢慢靠近。", "眼神放松了些。", "点点头表示认可。"],
            Self::Snail => &[
                "触角轻轻探了探。",
                "慢慢把壳往你这边挪。",
                "安静地表示很受用。",
            ],
            Self::Axolotl => &["小鳃抖了抖。", "软软靠近一点。", "漂着转了一圈。"],
            Self::Capybara => &[
                "舒服地叹了口气。",
                "慢慢靠近，还是很松弛。",
                "安静地接受抚摸。",
            ],
            Self::Cactus => &["小刺轻轻动了下。", "不动声色地开心。", "气场一下软了点。"],
            Self::Mushroom => &[
                "蘑菇帽轻轻晃了晃。",
                "开心地冒个小泡泡。",
                "轻轻贴近表示好评。",
            ],
            Self::Chonk => &["满意地滚了一小下。", "软乎乎地靠近。", "开心得微微颤了颤。"],
        }
    }

    pub(crate) fn teaser_lines(self) -> &'static [&'static str] {
        match self {
            Self::Cat => &["在旁边等你。", "耳朵动了动。"],
            Self::Fox => &["正歪头看着你。", "像知道你下一步。"],
            Self::Otter => &["叼着石子冒头。", "等你来摸一下。"],
            Self::Rabbit => &["轻轻跳了一下。", "就在这儿等你。"],
            Self::Owl => &["在输入框旁落座。", "安静看着你。"],
            Self::Dragon => &["像一簇暖火守着。", "喷了点小火星。"],
            Self::Ghost => &["礼貌地漂在一旁。", "挥了挥半透明小爪子。"],
            Self::Robot => &["进入待机。", "可爱系统在线。"],
            Self::Duck => &["在底栏轻轻晃着。", "嘎地一声招呼你。"],
            Self::Goose => &["昂着脖子盯着你。", "像在等你通过摸摸申请。"],
            Self::Blob => &["软软地趴着等你。", "缓慢地抖了抖。"],
            Self::Octopus => &["触手轻轻摆着。", "在旁边安静围观。"],
            Self::Penguin => &["带着一点凉意看着你。", "翅膀轻轻拍了拍。"],
            Self::Turtle => &["慢慢观察着。", "壳边轻轻动了下。"],
            Self::Snail => &["正慢慢探出触角。", "背着壳耐心等你。"],
            Self::Axolotl => &["小鳃轻轻摆动。", "漂在旁边等你。"],
            Self::Capybara => &["淡定地晒着太阳。", "看你忙完再说。"],
            Self::Cactus => &["稳稳立着。", "像个安静小守卫。"],
            Self::Mushroom => &["轻轻冒着。", "蘑菇帽晃了晃。"],
            Self::Chonk => &["软乎乎地躺着。", "团成一团等你。"],
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
    At,
}

impl BuddyEye {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Dot => "点点",
            Self::Spark => "星光",
            Self::Cross => "叉叉",
            Self::Wide => "圆眼",
            Self::Sleepy => "困倦",
            Self::At => "@",
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
            (Self::At, false) => "@",
            (Self::At, true) => "@",
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
    TinyDuck,
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
            Self::TinyDuck => "小鸭帽",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

    /// 稀有度专属的前缀装饰（用于 full sprite）
    pub(crate) fn sprite_prefix(self) -> Option<&'static str> {
        match self {
            Self::Legendary => Some("✦ "),
            _ => None,
        }
    }

    /// 稀有度专属的后缀装饰（用于 full sprite）
    pub(crate) fn sprite_suffix(self) -> Option<&'static str> {
        match self {
            Self::Epic => Some(" ✨"),
            Self::Legendary => Some(" ✦"),
            _ => None,
        }
    }

    /// 稀有度边框符号（用于 sprite 周围）
    pub(crate) fn frame_symbol(self) -> Option<&'static str> {
        match self {
            Self::Rare => Some("·"),
            Self::Epic => Some("✦"),
            Self::Legendary => Some("★"),
            _ => None,
        }
    }

    /// 窄屏视图的稀有度符号
    pub(crate) fn compact_symbol(self) -> &'static str {
        match self {
            Self::Common => "",
            Self::Uncommon => "◆",
            Self::Rare => "✦",
            Self::Epic => "★",
            Self::Legendary => "✧",
        }
    }

    /// 稀有度视觉特征描述（用于 status 文案）
    pub(crate) fn visual_trait(self) -> &'static str {
        match self {
            Self::Common => "普通外观",
            Self::Uncommon => "微光轮廓",
            Self::Rare => "星点边框",
            Self::Epic => "闪耀光环 + 专属标识",
            Self::Legendary => "传奇光效 + 专属标识",
        }
    }

    /// 稀有度专属的上方光晕行（用于 full sprite）
    pub(crate) fn aura_top(self) -> Option<&'static str> {
        match self {
            Self::Epic => Some("  .  ✨  .  "),
            Self::Legendary => Some(" ✧ ★  ✦  ★ ✧ "),
            _ => None,
        }
    }

    /// 稀有度专属的下方光晕行（用于 full sprite）
    pub(crate) fn aura_bottom(self) -> Option<&'static str> {
        match self {
            Self::Legendary => Some(" ✧    ✦    ✧ "),
            _ => None,
        }
    }

    /// 窄屏视图的前缀包裹符号
    pub(crate) fn compact_prefix(self) -> &'static str {
        match self {
            Self::Legendary => "「",
            _ => "",
        }
    }

    /// 窄屏视图的后缀包裹符号
    pub(crate) fn compact_suffix(self) -> &'static str {
        match self {
            Self::Legendary => "」",
            _ => "",
        }
    }

    /// 身份标识徽章（用于 identity line）
    pub(crate) fn identity_badge(self) -> Option<&'static str> {
        match self {
            Self::Epic => Some("◈"),
            Self::Legendary => Some("✦"),
            _ => None,
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
        let base_species = [
            BuddySpecies::Cat,
            BuddySpecies::Fox,
            BuddySpecies::Otter,
            BuddySpecies::Rabbit,
            BuddySpecies::Owl,
            BuddySpecies::Duck,
            BuddySpecies::Goose,
            BuddySpecies::Blob,
            BuddySpecies::Octopus,
            BuddySpecies::Penguin,
            BuddySpecies::Turtle,
            BuddySpecies::Snail,
            BuddySpecies::Axolotl,
            BuddySpecies::Capybara,
            BuddySpecies::Cactus,
            BuddySpecies::Mushroom,
            BuddySpecies::Chonk,
        ];
        let rare_species = [
            BuddySpecies::Dragon,
            BuddySpecies::Ghost,
            BuddySpecies::Robot,
        ];
        let species_choices = if rarity >= BuddyRarity::Rare {
            let mut all = base_species.to_vec();
            all.extend_from_slice(&rare_species);
            all
        } else {
            base_species.to_vec()
        };
        let species = species_choices[rng.random_range(0..species_choices.len())];
        let eye_choices = [
            BuddyEye::Dot,
            BuddyEye::Spark,
            BuddyEye::Cross,
            BuddyEye::Wide,
            BuddyEye::Sleepy,
            BuddyEye::At,
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
                BuddyHat::TinyDuck,
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
    Observed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyReactionKind {
    Hatch,
    Return,
    Pet,
    Teaser,
    Observe,
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
    pub(crate) full_layout: bool,
    pub(crate) pet_count: u32,
    pub(crate) last_action: Option<BuddyLastAction>,
    pub(crate) reaction: Option<BuddyReaction>,
    pub(crate) pet_started_at: Option<Instant>,
    pub(crate) pet_until: Option<Instant>,
    pub(crate) full_layout_until: Option<Instant>,
    tick_origin: Instant,
}

impl Default for BuddyState {
    fn default() -> Self {
        Self {
            visible: false,
            full_layout: false,
            pet_count: 0,
            last_action: None,
            reaction: None,
            pet_started_at: None,
            pet_until: None,
            full_layout_until: None,
            tick_origin: Instant::now(),
        }
    }
}

impl BuddyState {
    pub(crate) fn full_layout_active(&self) -> bool {
        self.full_layout_active_at(Instant::now())
    }

    pub(crate) fn full_layout_active_at(&self, now: Instant) -> bool {
        self.visible
            && (self.full_layout || self.full_layout_until.is_some_and(|until| now < until))
    }

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
                self.full_layout_until.filter(|until| now < *until),
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
