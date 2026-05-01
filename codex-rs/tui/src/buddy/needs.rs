//! 宠物需求与情绪系统。
//!
//! 需求值随时间衰减，通过互动补充。
//! 情绪由当前需求状态综合计算得出。

use std::time::Duration;
use std::time::Instant;

use ratatui::style::Stylize;
use ratatui::text::Span;

/// 需求衰减周期：每 TICK_DURATION 做一次衰减结算。
const NEEDS_TICK: Duration = Duration::from_secs(60);

/// 饥饱衰减速率（每秒）。
const HUNGER_DECAY_RATE: f32 = 0.0008;
/// 活力衰减速率（每秒）。
const ENERGY_DECAY_RATE: f32 = 0.0005;
/// 心情衰减速率（每秒）。
const HAPPINESS_DECAY_RATE: f32 = 0.0006;

/// 抚摸对心情的提升。
const PET_HAPPINESS_GAIN: f32 = 0.08;
/// 喂食对饥饱的提升。
const FEED_HUNGER_GAIN: f32 = 0.25;
/// 喂食对心情的小幅提升。
const FEED_HAPPINESS_GAIN: f32 = 0.03;
/// 玩耍对心情的提升。
const PLAY_HAPPINESS_GAIN: f32 = 0.18;
/// 玩耍对活力的消耗。
const PLAY_ENERGY_COST: f32 = 0.12;
/// 休息对活力的恢复。
const SLEEP_ENERGY_GAIN: f32 = 0.35;
/// 休息对饥饱的小幅消耗。
const SLEEP_HUNGER_COST: f32 = 0.03;

/// 情绪状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyMood {
    /// 心情和活力都好。
    Happy,
    /// 活力低。
    Sleepy,
    /// 饥饱低。
    Hungry,
    /// 活力高、心情好。
    Playful,
    /// 各项均衡。
    Content,
    /// 心情低。
    Lonely,
}

impl BuddyMood {
    /// 情绪对应的 emoji 指示符。
    pub(crate) fn indicator(self) -> &'static str {
        match self {
            Self::Happy => "☺",
            Self::Sleepy => "ｚｚ",
            Self::Hungry => "🍽",
            Self::Playful => "★",
            Self::Content => "～",
            Self::Lonely => "…",
        }
    }

    /// 情绪对应的色彩样式。
    pub(crate) fn style_span(self, text: String) -> Span<'static> {
        match self {
            Self::Happy => text.green(),
            Self::Sleepy => text.dim(),
            Self::Hungry => text.yellow(),
            Self::Playful => text.magenta().bold(),
            Self::Content => text.into(),
            Self::Lonely => text.dim().italic(),
        }
    }

    pub(crate) fn compact_icon(self) -> &'static str {
        match self {
            Self::Happy => "☺",
            Self::Sleepy => "z",
            Self::Hungry => "u",
            Self::Playful => "*",
            Self::Content => "~",
            Self::Lonely => ".",
        }
    }
}

/// 互动类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuddyInteraction {
    /// 抚摸。
    Pet,
    /// 喂食。
    Feed,
    /// 玩耍。
    Play,
    /// 休息。
    Sleep,
}

/// 宠物需求值。
#[derive(Clone, Debug)]
pub(crate) struct BuddyNeeds {
    /// 饱食度 0.0 ~ 1.0。
    pub hunger: f32,
    /// 活力值 0.0 ~ 1.0。
    pub energy: f32,
    /// 心情值 0.0 ~ 1.0。
    pub happiness: f32,
    /// 上次衰减结算时间。
    last_decay: Instant,
}

impl Default for BuddyNeeds {
    fn default() -> Self {
        Self {
            hunger: 0.8,
            energy: 0.9,
            happiness: 0.7,
            last_decay: Instant::now(),
        }
    }
}

impl BuddyNeeds {
    /// 用指定时间初始化（测试用）。
    pub(crate) fn with_time(now: Instant) -> Self {
        Self {
            last_decay: now,
            ..Default::default()
        }
    }

    /// 执行一次需求衰减（每分钟调用一次）。
    pub(crate) fn tick_decay(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_decay).as_secs_f32();
        if elapsed < NEEDS_TICK.as_secs_f32() {
            return;
        }
        self.apply_decay(elapsed);
        self.last_decay = now;
    }

    /// 用指定时间做衰减（测试用）。
    pub(crate) fn tick_decay_at(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.last_decay).as_secs_f32();
        if elapsed < NEEDS_TICK.as_secs_f32() {
            return;
        }
        self.apply_decay(elapsed);
        self.last_decay = now;
    }

    fn apply_decay(&mut self, elapsed_secs: f32) {
        self.hunger = (self.hunger - elapsed_secs * HUNGER_DECAY_RATE).clamp(0.0, 1.0);
        self.energy = (self.energy - elapsed_secs * ENERGY_DECAY_RATE).clamp(0.0, 1.0);
        self.happiness = (self.happiness - elapsed_secs * HAPPINESS_DECAY_RATE).clamp(0.0, 1.0);
    }

    /// 应用互动效果。
    pub(crate) fn apply_interaction(&mut self, interaction: BuddyInteraction) {
        match interaction {
            BuddyInteraction::Pet => {
                self.happiness = (self.happiness + PET_HAPPINESS_GAIN).min(1.0);
            }
            BuddyInteraction::Feed => {
                self.hunger = (self.hunger + FEED_HUNGER_GAIN).min(1.0);
                self.happiness = (self.happiness + FEED_HAPPINESS_GAIN).min(1.0);
            }
            BuddyInteraction::Play => {
                self.happiness = (self.happiness + PLAY_HAPPINESS_GAIN).min(1.0);
                self.energy = (self.energy - PLAY_ENERGY_COST).max(0.0);
            }
            BuddyInteraction::Sleep => {
                self.energy = (self.energy + SLEEP_ENERGY_GAIN).min(1.0);
                self.hunger = (self.hunger - SLEEP_HUNGER_COST).max(0.0);
            }
        }
    }

    /// 根据当前需求计算情绪。
    pub(crate) fn mood(&self) -> BuddyMood {
        if self.hunger < 0.2 {
            BuddyMood::Hungry
        } else if self.energy < 0.2 {
            BuddyMood::Sleepy
        } else if self.happiness < 0.2 {
            BuddyMood::Lonely
        } else if self.happiness > 0.7 && self.energy > 0.6 {
            BuddyMood::Happy
        } else if self.energy > 0.8 && self.happiness > 0.5 {
            BuddyMood::Playful
        } else {
            BuddyMood::Content
        }
    }

    /// 饥饱等级标签。
    pub(crate) fn hunger_label(&self) -> &'static str {
        if self.hunger > 0.8 {
            "饱"
        } else if self.hunger > 0.5 {
            "尚可"
        } else if self.hunger > 0.2 {
            "饿了"
        } else {
            "很饿"
        }
    }

    /// 活力等级标签。
    pub(crate) fn energy_label(&self) -> &'static str {
        if self.energy > 0.8 {
            "充沛"
        } else if self.energy > 0.5 {
            "还行"
        } else if self.energy > 0.2 {
            "疲惫"
        } else {
            "困了"
        }
    }

    /// 心情等级标签。
    pub(crate) fn happiness_label(&self) -> &'static str {
        if self.happiness > 0.8 {
            "开心"
        } else if self.happiness > 0.5 {
            "不错"
        } else if self.happiness > 0.2 {
            "低落"
        } else {
            "难过"
        }
    }
}

/// 需求进度条渲染（用于 full layout）。
pub(crate) fn needs_bar(value: f32, width: usize, filled: &str, empty: &str) -> String {
    let filled_count = (value * width as f32).round() as usize;
    let empty_count = width.saturating_sub(filled_count);
    format!(
        "{}{}",
        filled.repeat(filled_count),
        empty.repeat(empty_count)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn default_needs_are_healthy() {
        let needs = BuddyNeeds::default();
        assert!(needs.hunger > 0.5);
        assert!(needs.energy > 0.5);
        assert!(needs.happiness > 0.5);
    }

    #[test]
    fn mood_hungry_when_low_hunger() {
        let mut needs = BuddyNeeds::default();
        needs.hunger = 0.1;
        assert_eq!(needs.mood(), BuddyMood::Hungry);
    }

    #[test]
    fn mood_sleepy_when_low_energy() {
        let mut needs = BuddyNeeds::default();
        needs.energy = 0.1;
        assert_eq!(needs.mood(), BuddyMood::Sleepy);
    }

    #[test]
    fn mood_lonely_when_low_happiness() {
        let mut needs = BuddyNeeds::default();
        needs.happiness = 0.1;
        assert_eq!(needs.mood(), BuddyMood::Lonely);
    }

    #[test]
    fn mood_happy_when_high_happiness_and_energy() {
        let mut needs = BuddyNeeds::default();
        needs.happiness = 0.8;
        needs.energy = 0.7;
        assert_eq!(needs.mood(), BuddyMood::Happy);
    }

    #[test]
    fn mood_playful_when_very_high_energy() {
        let mut needs = BuddyNeeds::default();
        needs.energy = 0.9;
        needs.happiness = 0.6;
        assert_eq!(needs.mood(), BuddyMood::Playful);
    }

    #[test]
    fn feed_increases_hunger() {
        let mut needs = BuddyNeeds::default();
        needs.hunger = 0.3;
        needs.apply_interaction(BuddyInteraction::Feed);
        assert!(needs.hunger > 0.5);
    }

    #[test]
    fn play_increases_happiness_decreases_energy() {
        let mut needs = BuddyNeeds::default();
        let initial_energy = needs.energy;
        needs.apply_interaction(BuddyInteraction::Play);
        assert!(needs.happiness > 0.7);
        assert!(needs.energy < initial_energy);
    }

    #[test]
    fn sleep_increases_energy() {
        let mut needs = BuddyNeeds::default();
        needs.energy = 0.3;
        needs.apply_interaction(BuddyInteraction::Sleep);
        assert!(needs.energy > 0.6);
    }

    #[test]
    fn decay_reduces_values() {
        let mut needs = BuddyNeeds::default();
        let now = Instant::now();
        needs.last_decay = now - Duration::from_secs(300);
        needs.tick_decay_at(now);
        assert!(needs.hunger < 0.8);
        assert!(needs.energy < 0.9);
        assert!(needs.happiness < 0.7);
    }

    #[test]
    fn bar_rendering() {
        let bar = needs_bar(0.5, 4, "█", "░");
        assert_eq!(bar, "██░░");
    }

    #[test]
    fn hunger_labels() {
        let mut needs = BuddyNeeds::default();
        needs.hunger = 0.9;
        assert_eq!(needs.hunger_label(), "饱");
        needs.hunger = 0.6;
        assert_eq!(needs.hunger_label(), "尚可");
        needs.hunger = 0.3;
        assert_eq!(needs.hunger_label(), "饿了");
        needs.hunger = 0.1;
        assert_eq!(needs.hunger_label(), "很饿");
    }
}
