//! 后台分页的休眠计时；只记录最后一次离开前台的时间。
use std::time::{Duration, Instant};

pub const SLEEP_RECHECK: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct SleepTimer {
    background_since: Option<Instant>,
}

impl SleepTimer {
    pub fn update(&mut self, active: bool, now: Instant) {
        if active {
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
