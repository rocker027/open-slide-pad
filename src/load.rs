//! 一個網站分頁的載入狀態。
//!
//! wry 只回報「已開始顯示（commit）」與「完成」，沒有失敗事件。失敗只能由輪詢推斷：
//! App 還在等，WKWebView 的 `isLoading` 卻已經停了。遠端網頁不注入腳本，所以不從網頁端回報。

/// 連續幾次輪詢都看到原生端已停止，才認定這次載入已結束。
/// 完成事件經事件佇列送達，可能比輪詢晚一拍；只看一次會把成功誤判成失敗。
const QUIET_POLLS_TO_SETTLE: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadFailure {
    /// App 要求的載入沒有顯示任何內容就結束：連不上、逾時、網址無法解析。
    Unreachable,
    /// 網頁的處理程序被系統終止。
    Crashed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    /// App 發起（新分頁、網址列、重試）：沒有 commit 就結束即為失敗。
    Requested,
    /// 網頁自己發起（點連結、腳本導向）：沒有 commit 就結束屬正常，例如下載或被導覽規則擋下的外部連結。
    Navigating,
    /// 已有內容顯示；之後即使中斷，畫面上仍有可用的頁面。
    Committed,
    Failed(LoadFailure),
}

#[derive(Debug, Default)]
pub struct LoadWatch {
    phase: Phase,
    quiet_polls: u8,
}

impl LoadWatch {
    /// App 發起的載入。
    pub fn requested() -> Self {
        Self {
            phase: Phase::Requested,
            quiet_polls: 0,
        }
    }

    pub fn request(&mut self) {
        *self = Self::requested();
    }

    /// wry 的 Started：新頁面開始顯示，先前的失敗畫面也隨之結束。
    pub fn committed(&mut self) {
        self.enter(Phase::Committed);
    }

    /// wry 的 Finished；晚到的完成事件也會撤銷已推斷的失敗。
    pub fn finished(&mut self) {
        self.enter(Phase::Idle);
    }

    /// 使用者按了停止；之後原生端停止載入不算失敗。
    pub fn stopped(&mut self) {
        if self.is_loading() {
            self.enter(Phase::Idle);
        }
    }

    pub fn crashed(&mut self) {
        self.enter(Phase::Failed(LoadFailure::Crashed));
    }

    /// 每次輪詢傳入 WKWebView 的 `isLoading`；回傳 true 表示狀態改變、需要重畫。
    pub fn observe(&mut self, native_loading: bool) -> bool {
        match (self.phase, native_loading) {
            (Phase::Idle, true) => {
                self.enter(Phase::Navigating);
                true
            }
            (Phase::Requested | Phase::Navigating | Phase::Committed, true) => {
                self.quiet_polls = 0;
                false
            }
            (Phase::Requested | Phase::Navigating | Phase::Committed, false) => self.settle(),
            (Phase::Idle | Phase::Failed(_), _) => false,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(
            self.phase,
            Phase::Requested | Phase::Navigating | Phase::Committed
        )
    }

    pub fn failure(&self) -> Option<LoadFailure> {
        match self.phase {
            Phase::Failed(failure) => Some(failure),
            _ => None,
        }
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.quiet_polls = 0;
    }

    fn settle(&mut self) -> bool {
        self.quiet_polls += 1;
        if self.quiet_polls < QUIET_POLLS_TO_SETTLE {
            return false;
        }
        let ended = if self.phase == Phase::Requested {
            Phase::Failed(LoadFailure::Unreachable)
        } else {
            Phase::Idle
        };
        self.enter(ended);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_load_runs_from_request_to_finish_without_failing() {
        let mut load = LoadWatch::requested();
        assert!(load.is_loading());
        assert!(!load.observe(true));
        load.committed();
        assert!(!load.observe(true));
        load.finished();
        assert!(!load.is_loading() && load.failure().is_none());
        assert!(!load.observe(false));
    }

    #[test]
    fn a_requested_load_that_ends_without_committing_is_unreachable() {
        let mut load = LoadWatch::requested();
        assert!(!load.observe(true));
        // 第一次看到停止先不下結論：完成事件可能還在佇列裡。
        assert!(!load.observe(false));
        assert!(load.is_loading() && load.failure().is_none());
        assert!(load.observe(false));
        assert_eq!(load.failure(), Some(LoadFailure::Unreachable));
        assert!(!load.is_loading());
        // 失敗後不因後續輪詢自行恢復或重複通知。
        assert!(!load.observe(false) && !load.observe(true));
        assert_eq!(load.failure(), Some(LoadFailure::Unreachable));
    }

    #[test]
    fn a_late_finish_event_wins_over_a_single_quiet_poll_and_over_an_inferred_failure() {
        let mut load = LoadWatch::requested();
        assert!(!load.observe(false));
        load.finished();
        assert!(!load.observe(false) && !load.observe(false));
        assert!(load.failure().is_none());

        let mut late = LoadWatch::requested();
        assert!(!late.observe(false) && late.observe(false));
        late.finished();
        assert!(late.failure().is_none() && !late.is_loading());
    }

    #[test]
    fn quiet_polls_must_be_consecutive() {
        let mut load = LoadWatch::requested();
        assert!(!load.observe(false));
        assert!(!load.observe(true));
        assert!(!load.observe(false));
        assert!(load.failure().is_none());
    }

    #[test]
    fn page_initiated_navigation_shows_progress_but_never_becomes_a_failure() {
        let mut load = LoadWatch::default();
        assert!(load.observe(true));
        assert!(load.is_loading());
        // 下載或被導覽規則擋下的外部連結：沒有 commit 就結束，原頁面仍在，不顯示錯誤。
        assert!(!load.observe(false) && load.observe(false));
        assert!(!load.is_loading() && load.failure().is_none());
    }

    #[test]
    fn an_interrupted_page_keeps_its_content_instead_of_showing_an_error() {
        let mut load = LoadWatch::requested();
        load.committed();
        assert!(!load.observe(false) && load.observe(false));
        assert!(!load.is_loading() && load.failure().is_none());
    }

    #[test]
    fn stop_is_not_a_failure_and_only_applies_while_loading() {
        let mut load = LoadWatch::requested();
        load.stopped();
        assert!(!load.observe(false) && !load.observe(false));
        assert!(!load.is_loading() && load.failure().is_none());

        let mut failed = LoadWatch::requested();
        assert!(!failed.observe(false) && failed.observe(false));
        failed.stopped();
        assert_eq!(failed.failure(), Some(LoadFailure::Unreachable));
    }

    #[test]
    fn crash_and_retry_and_back_navigation_leave_the_failed_state() {
        let mut load = LoadWatch::default();
        load.crashed();
        assert_eq!(load.failure(), Some(LoadFailure::Crashed));
        load.request();
        assert!(load.is_loading() && load.failure().is_none());

        let mut back = LoadWatch::requested();
        assert!(!back.observe(false) && back.observe(false));
        // 失敗畫面上按「上一頁」：舊頁面 commit 後失敗畫面結束。
        back.committed();
        assert!(back.is_loading() && back.failure().is_none());
    }
}
