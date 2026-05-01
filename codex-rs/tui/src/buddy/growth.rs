//! 宠物成长系统。
//!
//! 通过互动积累经验值，提升等级，解锁里程碑。

use std::time::Instant;

use super::needs::BuddyInteraction;

/// 每次抚摸获得的经验。
const PET_XP: u32 = 5;
/// 每次喂食获得的经验。
const FEED_XP: u32 = 8;
/// 每次玩耍获得的经验。
const PLAY_XP: u32 = 10;
/// 每次休息获得的经验。
const SLEEP_XP: u32 = 3;

/// 升级所需经验公式：level * LEVEL_XP_BASE。
const LEVEL_XP_BASE: u32 = 100;

/// 里程碑定义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Milestone {
    /// 首次孵化。
    FirstHatch,
    /// 首次抚摸。
    FirstPet,
    /// 首次喂食。
    FirstFeed,
    /// 首次玩耍。
    FirstPlay,
    /// 首次休息。
    FirstSleep,
    /// 抚摸 10 次。
    Pet10,
    /// 抚摸 50 次。
    Pet50,
    /// 抚摸 100 次。
    Pet100,
    /// 总互动 50 次。
    Total50,
    /// 总互动 200 次。
    Total200,
    /// 达到 5 级。
    Level5,
    /// 达到 10 级。
    Level10,
    /// 连续 3 天出现。
    Streak3,
}

impl Milestone {
    /// 里程碑名称。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FirstHatch => "初见",
            Self::FirstPet => "初次触摸",
            Self::FirstFeed => "第一餐",
            Self::FirstPlay => "初次游戏",
            Self::FirstSleep => "第一次小憩",
            Self::Pet10 => "亲密接触 ×10",
            Self::Pet50 => "超级黏人 ×50",
            Self::Pet100 => "灵魂伴侣 ×100",
            Self::Total50 => "互动达人 ×50",
            Self::Total200 => "互动大师 ×200",
            Self::Level5 => "成长 · Lv.5",
            Self::Level10 => "成熟 · Lv.10",
            Self::Streak3 => "三日之约",
        }
    }

    /// 里程碑描述。
    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::FirstHatch => "首次遇见你的小伙伴",
            Self::FirstPet => "第一次抚摸你的小伙伴",
            Self::FirstFeed => "第一次喂食",
            Self::FirstPlay => "第一次和小伙伴玩耍",
            Self::FirstSleep => "第一次让小伙伴休息",
            Self::Pet10 => "累计抚摸 10 次",
            Self::Pet50 => "累计抚摸 50 次",
            Self::Pet100 => "累计抚摸 100 次",
            Self::Total50 => "累计互动 50 次",
            Self::Total200 => "累计互动 200 次",
            Self::Level5 => "达到 5 级",
            Self::Level10 => "达到 10 级",
            Self::Streak3 => "连续 3 天与小伙伴相见",
        }
    }
}

/// 成长状态。
#[derive(Clone, Debug)]
pub(crate) struct BuddyGrowth {
    /// 当前等级。
    pub level: u32,
    /// 当前等级已获得的经验。
    pub xp: u32,
    /// 累计抚摸次数。
    pub pet_count: u32,
    /// 累计喂食次数。
    pub feed_count: u32,
    /// 累计玩耍次数。
    pub play_count: u32,
    /// 累计休息次数。
    pub sleep_count: u32,
    /// 已达成的里程碑。
    milestones: Vec<Milestone>,
    /// 连续出现天数。
    pub streak_days: u32,
    /// 上次出现日期（简化为天数戳）。
    pub last_seen_day: u64,
    /// 孵化时间。
    pub hatched_at: Instant,
    /// 是否已经完成孵化。
    hatched: bool,
}

impl Default for BuddyGrowth {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            pet_count: 0,
            feed_count: 0,
            play_count: 0,
            sleep_count: 0,
            milestones: Vec::new(),
            streak_days: 0,
            last_seen_day: 0,
            hatched_at: Instant::now(),
            hatched: false,
        }
    }
}

impl BuddyGrowth {
    /// 用指定孵化时间初始化。
    pub(crate) fn with_hatch_time(now: Instant) -> Self {
        Self {
            hatched_at: now,
            ..Default::default()
        }
    }

    /// 记录一次互动，返回新达成的里程碑列表。
    pub(crate) fn record_interaction(&mut self, interaction: BuddyInteraction) -> Vec<Milestone> {
        let xp = match interaction {
            BuddyInteraction::Pet => {
                self.pet_count += 1;
                PET_XP
            }
            BuddyInteraction::Feed => {
                self.feed_count += 1;
                FEED_XP
            }
            BuddyInteraction::Play => {
                self.play_count += 1;
                PLAY_XP
            }
            BuddyInteraction::Sleep => {
                self.sleep_count += 1;
                SLEEP_XP
            }
        };
        self.gain_xp(xp);
        self.check_milestones()
    }

    /// 记录孵化事件。
    pub(crate) fn record_hatch(&mut self) -> Vec<Milestone> {
        self.hatched_at = Instant::now();
        self.hatched = true;
        self.check_milestones()
    }

    /// 记录每日出现。
    pub(crate) fn record_daily_visit(&mut self, today: u64) -> Vec<Milestone> {
        if today == 0 {
            return Vec::new();
        }
        if self.last_seen_day == 0 {
            self.streak_days = 1;
        } else if today == self.last_seen_day + 1 {
            self.streak_days += 1;
        } else if today > self.last_seen_day {
            self.streak_days = 1;
        }
        self.last_seen_day = today;
        self.check_milestones()
    }

    fn gain_xp(&mut self, amount: u32) {
        self.xp += amount;
        let needed = self.xp_needed();
        while self.xp >= needed {
            self.xp -= needed;
            self.level += 1;
        }
    }

    /// 当前升级所需经验。
    pub(crate) fn xp_needed(&self) -> u32 {
        self.level * LEVEL_XP_BASE
    }

    /// 经验进度百分比。
    pub(crate) fn xp_progress(&self) -> f32 {
        let needed = self.xp_needed();
        if needed == 0 {
            return 1.0;
        }
        self.xp as f32 / needed as f32
    }

    /// 总互动次数。
    pub(crate) fn total_interactions(&self) -> u32 {
        self.pet_count + self.feed_count + self.play_count + self.sleep_count
    }

    /// 当前已连续出现天数。
    pub(crate) fn streak_days(&self) -> u32 {
        self.streak_days
    }

    fn check_milestones(&mut self) -> Vec<Milestone> {
        let candidates = [
            (
                self.hatched && !self.has_milestone(Milestone::FirstHatch),
                Milestone::FirstHatch,
            ),
            (
                self.pet_count >= 1 && !self.has_milestone(Milestone::FirstPet),
                Milestone::FirstPet,
            ),
            (
                self.feed_count >= 1 && !self.has_milestone(Milestone::FirstFeed),
                Milestone::FirstFeed,
            ),
            (
                self.play_count >= 1 && !self.has_milestone(Milestone::FirstPlay),
                Milestone::FirstPlay,
            ),
            (
                self.sleep_count >= 1 && !self.has_milestone(Milestone::FirstSleep),
                Milestone::FirstSleep,
            ),
            (
                self.pet_count >= 10 && !self.has_milestone(Milestone::Pet10),
                Milestone::Pet10,
            ),
            (
                self.pet_count >= 50 && !self.has_milestone(Milestone::Pet50),
                Milestone::Pet50,
            ),
            (
                self.pet_count >= 100 && !self.has_milestone(Milestone::Pet100),
                Milestone::Pet100,
            ),
            (
                self.total_interactions() >= 50 && !self.has_milestone(Milestone::Total50),
                Milestone::Total50,
            ),
            (
                self.total_interactions() >= 200 && !self.has_milestone(Milestone::Total200),
                Milestone::Total200,
            ),
            (
                self.level >= 5 && !self.has_milestone(Milestone::Level5),
                Milestone::Level5,
            ),
            (
                self.level >= 10 && !self.has_milestone(Milestone::Level10),
                Milestone::Level10,
            ),
            (
                self.streak_days >= 3 && !self.has_milestone(Milestone::Streak3),
                Milestone::Streak3,
            ),
        ];

        let mut new_milestones = Vec::new();
        for (condition, milestone) in candidates {
            if condition {
                self.milestones.push(milestone);
                new_milestones.push(milestone);
            }
        }
        new_milestones
    }

    fn has_milestone(&self, milestone: Milestone) -> bool {
        self.milestones.contains(&milestone)
    }

    /// 获取所有已达成里程碑。
    pub(crate) fn milestones(&self) -> &[Milestone] {
        &self.milestones
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn starts_at_level_1() {
        let growth = BuddyGrowth::default();
        assert_eq!(growth.level, 1);
        assert_eq!(growth.xp, 0);
    }

    #[test]
    fn pet_grants_xp() {
        let mut growth = BuddyGrowth::default();
        growth.record_interaction(BuddyInteraction::Pet);
        assert_eq!(growth.xp, PET_XP);
        assert_eq!(growth.pet_count, 1);
    }

    #[test]
    fn leveling_up() {
        let mut growth = BuddyGrowth::default();
        // Level 1 needs 100 XP
        for _ in 0..20 {
            growth.record_interaction(BuddyInteraction::Play);
        }
        // 20 * 10 = 200 XP, should be level 2 or 3
        assert!(growth.level >= 2);
    }

    #[test]
    fn first_pet_milestone() {
        let mut growth = BuddyGrowth::default();
        let milestones = growth.record_interaction(BuddyInteraction::Pet);
        assert!(milestones.contains(&Milestone::FirstPet));
    }

    #[test]
    fn hatch_records_first_hatch_milestone() {
        let mut growth = BuddyGrowth::default();
        let milestones = growth.record_hatch();
        assert!(milestones.contains(&Milestone::FirstHatch));
    }

    #[test]
    fn pet10_milestone() {
        let mut growth = BuddyGrowth::default();
        let mut got_pet10 = false;
        for _ in 0..10 {
            let milestones = growth.record_interaction(BuddyInteraction::Pet);
            if milestones.contains(&Milestone::Pet10) {
                got_pet10 = true;
            }
        }
        assert!(got_pet10);
    }

    #[test]
    fn streak_tracking() {
        let mut growth = BuddyGrowth::default();
        growth.record_daily_visit(1);
        assert_eq!(growth.streak_days, 1);
        growth.record_daily_visit(2);
        assert_eq!(growth.streak_days, 2);
        growth.record_daily_visit(3);
        assert_eq!(growth.streak_days, 3);
        // Gap resets
        growth.record_daily_visit(10);
        assert_eq!(growth.streak_days, 1);
    }

    #[test]
    fn xp_progress_calculation() {
        let mut growth = BuddyGrowth::default();
        growth.xp = 50;
        assert!((growth.xp_progress() - 0.5).abs() < 0.01);
    }

    #[test]
    fn total_interactions_count() {
        let mut growth = BuddyGrowth::default();
        growth.record_interaction(BuddyInteraction::Pet);
        growth.record_interaction(BuddyInteraction::Feed);
        growth.record_interaction(BuddyInteraction::Play);
        growth.record_interaction(BuddyInteraction::Sleep);
        assert_eq!(growth.total_interactions(), 4);
    }
}
