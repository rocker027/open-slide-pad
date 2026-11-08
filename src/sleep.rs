//! 分頁休眠計時；選取中的網站可選擇在側欄收起時一併計時。
use std::time::{Duration, Instant};

pub const SLEEP_RECHECK: Duration = Duration::from_secs(30);

/// 只有選取中的網站需要保護；啟用收起後休眠時，保護隨側欄可見狀態切換。
pub fn keep_awake(selected: bool, visible: bool, sleep_when_hidden: bool) -> bool {
    selected && (visible || !sleep_when_hidden)
}

#[derive(Default)]
pub struct SleepTimer {
    background_since: Option<Instant>,
}

impl SleepTimer {
    pub fn update(&mut self, kept_awake: bool, now: Instant) {
        if kept_awake {
            self.background_since = None;
        } else {
            self.background_since.get_or_insert(now);
        }
    }

    pub fn deadline(&self, minutes: u16) -> Option<Instant> {
        if minutes == 0 {
            return None;
        }
        self.background_since?
            .checked_add(Duration::from_secs(u64::from(minutes) * 60))
    }

    pub fn due(&self, minutes: u16, now: Instant) -> bool {
        self.deadline(minutes)
            .is_some_and(|deadline| now >= deadline)
    }

    pub fn next_check(&self, minutes: u16, now: Instant) -> Option<Instant> {
        self.deadline(minutes).map(|deadline| {
            if deadline <= now {
                now + SLEEP_RECHECK
            } else {
                deadline
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_selected_tab_only_counts_down_after_opt_in() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        for visible in [true, false] {
            timer.update(keep_awake(true, visible, false), now);
            assert_eq!(timer.next_check(5, now), None);
        }
        timer.update(keep_awake(true, true, true), now);
        assert_eq!(timer.deadline(5), None);
        let hidden = now + Duration::from_secs(20);
        timer.update(keep_awake(true, false, true), hidden);
        assert_eq!(
            timer.next_check(5, hidden),
            Some(hidden + Duration::from_secs(300))
        );
        // 重複收起或背景事件不能把倒數往後延。
        timer.update(
            keep_awake(true, false, true),
            hidden + Duration::from_secs(200),
        );
        assert!(!timer.due(5, hidden + Duration::from_secs(299)));
        assert!(timer.due(5, hidden + Duration::from_secs(300)));
        assert_eq!(timer.next_check(0, hidden), None);
        assert!(!timer.due(0, hidden + Duration::from_secs(1000)));
    }

    #[test]
    fn reopening_or_disabling_hidden_sleep_cancels_the_selected_countdown() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.update(keep_awake(true, false, true), now);
        timer.update(keep_awake(true, true, true), now + Duration::from_secs(200));
        assert_eq!(timer.deadline(5), None);
        let hidden_again = now + Duration::from_secs(240);
        timer.update(keep_awake(true, false, true), hidden_again);
        assert!(!timer.due(5, now + Duration::from_secs(300)));
        assert_eq!(
            timer.deadline(5),
            Some(hidden_again + Duration::from_secs(300))
        );
        timer.update(
            keep_awake(true, false, false),
            now + Duration::from_secs(250),
        );
        assert_eq!(timer.deadline(5), None);
    }

    #[test]
    fn background_countdown_survives_visibility_and_hidden_sleep_changes() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        for (offset, visible, enabled) in [
            (0, true, false),
            (30, false, true),
            (60, true, true),
            (90, false, false),
        ] {
            timer.update(
                keep_awake(false, visible, enabled),
                now + Duration::from_secs(offset),
            );
            assert_eq!(timer.deadline(5), Some(now + Duration::from_secs(300)));
        }
    }

    #[test]
    fn only_background_tabs_sleep_at_the_configured_deadline() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.update(true, now);
        assert!(!timer.due(15, now + Duration::from_secs(3600)));
        timer.update(false, now);
        timer.update(false, now + Duration::from_secs(600));
        assert!(!timer.due(15, now + Duration::from_secs(899)));
        assert!(timer.due(15, now + Duration::from_secs(900)));
        assert!(!timer.due(0, now + Duration::from_secs(3600)));
        assert!(timer.due(5, now + Duration::from_secs(300)));
        assert!(!timer.due(30, now + Duration::from_secs(900)));
    }

    #[test]
    fn reselecting_a_tab_resets_its_next_background_period() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.update(false, now);
        timer.update(true, now + Duration::from_secs(800));
        timer.update(false, now + Duration::from_secs(900));
        assert!(!timer.due(15, now + Duration::from_secs(1799)));
        assert!(timer.due(15, now + Duration::from_secs(1800)));
    }

    #[test]
    fn expired_or_disabled_timers_do_not_busy_loop() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.update(false, now);
        assert_eq!(
            timer.next_check(15, now),
            Some(now + Duration::from_secs(900))
        );
        let later = now + Duration::from_secs(1000);
        assert_eq!(timer.next_check(15, later), Some(later + SLEEP_RECHECK));
        assert_eq!(timer.next_check(0, later), None);
    }
}
