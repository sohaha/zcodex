//! 宠物日记系统。
//!
//! 记录宠物生命中的关键时刻，供玩家回顾。

use std::time::Instant;

use super::growth::Milestone;

/// 日记条目。
#[derive(Clone, Debug)]
pub(crate) struct JournalEntry {
    /// 发生时间（相对于孵化的秒数）。
    pub elapsed_secs: u64,
    /// 事件内容。
    pub event: JournalEvent,
}

/// 日记事件类型。
#[derive(Clone, Debug)]
pub(crate) enum JournalEvent {
    /// 孵化。
    Hatched,
    /// 获得新里程碑。
    MilestoneReached(Milestone),
    /// 升级。
    LevelUp { level: u32 },
    /// 连续出现天数。
    StreakUpdate { days: u32 },
    /// 特殊时刻（由外部触发）。
    Special(String),
}

impl JournalEvent {
    /// 事件描述文本。
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Hatched => "来到了这个世界。".to_string(),
            Self::MilestoneReached(m) => format!("达成了里程碑「{}」。", m.label()),
            Self::LevelUp { level } => format!("升级到了 Lv.{level}！"),
            Self::StreakUpdate { days } => format!("连续出现了 {days} 天。"),
            Self::Special(text) => text.clone(),
        }
    }
}

/// 宠物日记。
#[derive(Clone, Debug)]
pub(crate) struct BuddyJournal {
    /// 所有日记条目。
    entries: Vec<JournalEntry>,
    /// 孵化时间。
    hatched_at: Instant,
}

impl Default for BuddyJournal {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            hatched_at: Instant::now(),
        }
    }
}

impl BuddyJournal {
    /// 用指定孵化时间初始化。
    pub(crate) fn with_hatch_time(now: Instant) -> Self {
        Self {
            hatched_at: now,
            ..Default::default()
        }
    }

    /// 记录一条日记。
    pub(crate) fn record(&mut self, event: JournalEvent) {
        let elapsed = self.hatched_at.elapsed().as_secs();
        self.entries.push(JournalEntry {
            elapsed_secs: elapsed,
            event,
        });
    }

    /// 记录里程碑。
    pub(crate) fn record_milestone(&mut self, milestone: Milestone) {
        self.record(JournalEvent::MilestoneReached(milestone));
    }

    /// 记录升级。
    pub(crate) fn record_level_up(&mut self, level: u32) {
        self.record(JournalEvent::LevelUp { level });
    }

    /// 记录连续出现。
    pub(crate) fn record_streak(&mut self, days: u32) {
        self.record(JournalEvent::StreakUpdate { days });
    }

    /// 获取所有日记条目。
    pub(crate) fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// 获取最近 N 条日记。
    pub(crate) fn recent(&self, n: usize) -> &[JournalEntry] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }

    /// 日记总条数。
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// 日记是否为空。
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 格式化日记条目为可读文本。
pub(crate) fn format_entry(entry: &JournalEntry) -> String {
    let hours = entry.elapsed_secs / 3600;
    let minutes = (entry.elapsed_secs % 3600) / 60;
    let time_str = if hours > 0 {
        format!("{hours}时{minutes}分")
    } else {
        format!("{minutes}分")
    };
    format!("[{time_str}] {}", entry.event.description())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn journal_records_entries() {
        let mut journal = BuddyJournal::default();
        journal.record(JournalEvent::Hatched);
        assert_eq!(journal.len(), 1);
    }

    #[test]
    fn journal_recent_limit() {
        let mut journal = BuddyJournal::default();
        for i in 0..10 {
            journal.record(JournalEvent::LevelUp { level: i + 1 });
        }
        let recent = journal.recent(3);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn journal_not_empty() {
        let mut journal = BuddyJournal::default();
        assert!(journal.is_empty());
        journal.record(JournalEvent::Hatched);
        assert!(!journal.is_empty());
    }

    #[test]
    fn format_entry_shows_time() {
        let entry = JournalEntry {
            elapsed_secs: 3661,
            event: JournalEvent::Hatched,
        };
        let formatted = format_entry(&entry);
        assert!(formatted.contains("1时1分"));
    }

    #[test]
    fn milestone_event_description() {
        let event = JournalEvent::MilestoneReached(Milestone::FirstPet);
        assert!(event.description().contains("初次触摸"));
    }

    #[test]
    fn level_up_event_description() {
        let event = JournalEvent::LevelUp { level: 5 };
        assert!(event.description().contains("Lv.5"));
    }
}
