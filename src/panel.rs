use crate::model::Side;
use std::time::{Duration, Instant};

pub const MIN_WIDTH: f64 = 360.0;
pub const MAX_WIDTH: f64 = 960.0;
pub const MIN_HEIGHT: f64 = 320.0;
pub const MAX_HEIGHT: f64 = 10_000.0;
pub const MARGIN: f64 = 8.0;
/// 游標停在邊緣多久才滑出。
pub const EDGE_DWELL: Duration = Duration::from_millis(180);
/// 游標離開面板多久才收合。
pub const HIDE_DELAY: Duration = Duration::from_millis(650);
/// 滑出動畫與拖曳調整大小期間的更新間隔。
pub const ANIMATION_TICK: Duration = Duration::from_millis(8);
/// 輪詢游標位置與導覽狀態的間隔。
pub const POLL_TICK: Duration = Duration::from_millis(32);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    pub left: f64,
    pub bottom: f64,
    pub width: f64,
    pub height: f64,
}

impl Frame {
    pub fn contains(self, point: (f64, f64)) -> bool {
        point.0 >= self.left
            && point.0 <= self.left + self.width
            && point.1 >= self.bottom
            && point.1 <= self.bottom + self.height
    }
    pub fn at_edge(self, point: (f64, f64), side: Side) -> bool {
        self.contains(point)
            && match side {
                Side::Left => point.0 <= self.left + 2.0,
                Side::Right => point.0 >= self.left + self.width - 2.0,
            }
    }
    pub fn panel(self, width: f64, height: Option<f64>, top_offset: f64, side: Side) -> Self {
        let available_width = (self.width - MARGIN * 2.0).max(1.0);
        let available_height = (self.height - MARGIN * 2.0).max(1.0);
        let width = width.clamp(MIN_WIDTH, MAX_WIDTH).min(available_width);
        let height = height
            .map(|height| height.clamp(MIN_HEIGHT, MAX_HEIGHT))
            .unwrap_or(available_height)
            .min(available_height);
        let top_offset = top_offset.clamp(MARGIN, (self.height - height - MARGIN).max(MARGIN));
        Self {
            left: if side == Side::Left {
                self.left + MARGIN
            } else {
                self.left + self.width - width - MARGIN
            },
            bottom: self.bottom + self.height - top_offset - height,
            width,
            height,
        }
    }

    /// 保留拖曳後的頂端位置，並將最終尺寸限制在螢幕內、重新貼齊指定側邊。
    pub fn constrain_panel(self, proposed: Frame, side: Side) -> Frame {
        let top_offset = self.bottom + self.height - proposed.bottom - proposed.height;
        self.panel(proposed.width, Some(proposed.height), top_offset, side)
    }
}

#[derive(Default)]
pub struct EdgeTrigger {
    since: Option<Instant>,
    latched: bool,
}
impl EdgeTrigger {
    pub fn reset_until_leave(&mut self) {
        self.since = None;
        self.latched = true;
    }
    pub fn update(&mut self, at_edge: bool, now: Instant) -> bool {
        if !at_edge {
            self.since = None;
            self.latched = false;
            return false;
        }
        if self.latched {
            return false;
        }
        let start = *self.since.get_or_insert(now);
        if now.duration_since(start) >= EDGE_DWELL {
            self.latched = true;
            return true;
        }
        false
    }
}

/// 事件迴圈目前需要輪詢的狀態，用來決定下次喚醒前可以睡多久。
#[derive(Clone, Copy, Debug, Default)]
pub struct Activity {
    /// smoke 測試靠輪詢推進。
    pub probing: bool,
    /// 動畫或拖曳調整大小進行中。
    pub animating: bool,
    pub visible: bool,
    pub hot_edge: bool,
}

impl Activity {
    /// None 代表沒有需要輪詢的狀態，等事件（快捷鍵、選單列）喚醒即可。
    pub fn wake_interval(self) -> Option<Duration> {
        if self.animating {
            Some(ANIMATION_TICK)
        } else if self.probing || self.visible || self.hot_edge {
            Some(POLL_TICK)
        } else {
            None
        }
    }
}

/// 面板由誰開啟：碰觸邊緣，或使用者明確要求（快捷鍵、選單列、App 選單）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpenedBy {
    Edge,
    Request,
}

/// 面板開啟後何時自動收合；釘選與設定面板開啟期間由呼叫端以 `held` 暫停。
///
/// 使用者要求開啟的面板，游標要進出一次才收合，避免剛開啟就消失。
/// 邊緣開啟時游標一定在面板外（邊緣與面板之間隔著 margin），所以把邊緣觸發區視為面板的一部分：
/// 停在邊緣不收合，離開邊緣又沒進面板就照常倒數。否則自訂高度或多螢幕接縫下面板永遠不會收合。
#[derive(Default)]
pub struct AutoHide {
    by_edge: bool,
    entered: bool,
    outside_since: Option<Instant>,
}

impl AutoHide {
    pub fn opened(&mut self, by: OpenedBy) {
        let by_edge = by == OpenedBy::Edge;
        *self = Self {
            by_edge,
            entered: by_edge,
            outside_since: None,
        };
    }

    /// 拖曳調整大小期間不倒數。
    pub fn pause(&mut self) {
        self.outside_since = None;
    }

    /// 回傳 true 表示應該收合。`inside` 為游標在面板內，`at_edge` 為游標在邊緣觸發區。
    pub fn update(&mut self, inside: bool, at_edge: bool, held: bool, now: Instant) -> bool {
        if inside || (self.by_edge && at_edge) {
            self.entered = true;
            self.outside_since = None;
            return false;
        }
        if held {
            // 放開後重新倒數，不沿用釘選或開啟設定之前的離開時間。
            self.outside_since = None;
            return false;
        }
        if !self.entered {
            return false;
        }
        let start = *self.outside_since.get_or_insert(now);
        now.duration_since(start) > HIDE_DELAY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn event_loop_sleeps_when_hidden_without_hot_edge_and_speeds_up_for_animation() {
        let idle = Activity::default();
        assert_eq!(idle.wake_interval(), None);
        for polling in [
            Activity {
                hot_edge: true,
                ..idle
            },
            Activity {
                visible: true,
                ..idle
            },
            Activity {
                probing: true,
                ..idle
            },
        ] {
            assert_eq!(polling.wake_interval(), Some(POLL_TICK), "{polling:?}");
        }
        // 收合動畫進行時面板已標為不可見，仍須以動畫間隔更新到結束。
        assert_eq!(
            Activity {
                animating: true,
                ..idle
            }
            .wake_interval(),
            Some(ANIMATION_TICK)
        );
        assert!(ANIMATION_TICK < POLL_TICK);
    }

    #[test]
    fn requested_panel_stays_until_pointer_enters_and_then_leaves() {
        let start = Instant::now();
        let mut hide = AutoHide::default();
        hide.opened(OpenedBy::Request);
        // 快捷鍵開啟後游標可能在任何地方，包含邊緣；沒進過面板就不收合。
        assert!(!hide.update(false, true, false, after(start, 60_000)));
        assert!(!hide.update(false, false, false, after(start, 120_000)));
        assert!(!hide.update(true, false, false, after(start, 120_100)));
        assert!(!hide.update(false, false, false, after(start, 120_200)));
        assert!(!hide.update(false, false, false, after(start, 120_850)));
        assert!(hide.update(false, false, false, after(start, 120_851)));
    }

    #[test]
    fn edge_opened_panel_hides_when_pointer_leaves_without_ever_entering() {
        let start = Instant::now();
        let mut hide = AutoHide::default();
        hide.opened(OpenedBy::Edge);
        // 自訂高度或多螢幕接縫：游標從邊緣直接離開，沒有穿過面板。
        assert!(!hide.update(false, true, false, after(start, 100)));
        assert!(!hide.update(false, false, false, after(start, 200)));
        assert!(!hide.update(false, false, false, after(start, 850)));
        assert!(hide.update(false, false, false, after(start, 851)));
    }

    #[test]
    fn edge_opened_panel_stays_while_pointer_rests_at_edge_or_crosses_the_margin() {
        let start = Instant::now();
        let mut hide = AutoHide::default();
        hide.opened(OpenedBy::Edge);
        assert!(!hide.update(false, true, false, after(start, 60_000)));
        // 邊緣到面板之間有 margin，穿越時短暫在兩者之外。
        assert!(!hide.update(false, false, false, after(start, 60_010)));
        assert!(!hide.update(true, false, false, after(start, 60_020)));
        assert!(!hide.update(false, true, false, after(start, 90_000)));
        assert!(!hide.update(false, false, false, after(start, 90_010)));
        assert!(hide.update(false, false, false, after(start, 90_700)));
    }

    #[test]
    fn pinned_or_overlay_holds_the_panel_and_restarts_the_countdown() {
        let start = Instant::now();
        let mut hide = AutoHide::default();
        hide.opened(OpenedBy::Request);
        assert!(!hide.update(true, false, false, start));
        assert!(!hide.update(false, false, false, after(start, 100)));
        assert!(!hide.update(false, false, true, after(start, 5_000)));
        // 放開後重新倒數，不沿用釘選前的離開時間。
        assert!(!hide.update(false, false, false, after(start, 5_100)));
        assert!(hide.update(false, false, false, after(start, 5_800)));
    }

    #[test]
    fn pausing_for_resize_restarts_the_countdown() {
        let start = Instant::now();
        let mut hide = AutoHide::default();
        hide.opened(OpenedBy::Request);
        assert!(!hide.update(true, false, false, start));
        assert!(!hide.update(false, false, false, after(start, 100)));
        hide.pause();
        // 沒有暫停的話，離開已 700 ms，這裡就會收合。
        assert!(!hide.update(false, false, false, after(start, 800)));
        assert!(hide.update(false, false, false, after(start, 1_500)));
    }

    #[test]
    fn edge_zone_counts_as_panel_only_for_edge_opened_panels() {
        let start = Instant::now();
        let mut hide = AutoHide::default();
        hide.opened(OpenedBy::Request);
        assert!(!hide.update(true, false, false, start));
        // 快捷鍵開啟後移到邊緣停留，視為已離開面板。
        assert!(!hide.update(false, true, false, after(start, 100)));
        assert!(hide.update(false, true, false, after(start, 800)));
    }
    #[test]
    fn positions_on_negative_coordinate_display_and_clamps_width() {
        let screen = Frame {
            left: -1440.0,
            bottom: -200.0,
            width: 1440.0,
            height: 900.0,
        };
        assert_eq!(
            screen.panel(520.0, None, 8.0, Side::Right),
            Frame {
                left: -528.0,
                bottom: -192.0,
                width: 520.0,
                height: 884.0
            }
        );
        assert!(screen.at_edge((-1439.0, 0.0), Side::Left));
        assert!(!screen.at_edge((-1439.0, 901.0), Side::Left));
        assert!(screen.panel(2000.0, None, 8.0, Side::Right).left >= screen.left);
    }

    #[test]
    fn resizing_keeps_requested_top_and_reanchors_both_screen_edges() {
        let screen = Frame {
            left: -1440.0,
            bottom: -200.0,
            width: 1440.0,
            height: 900.0,
        };
        let proposed = Frame {
            left: -600.0,
            bottom: 100.0,
            width: 680.0,
            height: 540.0,
        };
        for (side, left) in [(Side::Left, -1432.0), (Side::Right, -688.0)] {
            assert_eq!(
                screen.constrain_panel(proposed, side),
                Frame { left, ..proposed }
            );
            assert_eq!(
                screen.panel(680.0, Some(540.0), 60.0, side),
                screen.constrain_panel(proposed, side)
            );
        }
    }

    #[test]
    fn resizing_clamps_width_height_and_top_or_bottom_overflow() {
        let screen = Frame {
            left: 100.0,
            bottom: 50.0,
            width: 1600.0,
            height: 1200.0,
        };
        assert_eq!(
            screen.panel(40.0, Some(20.0), 0.0, Side::Left),
            Frame {
                left: 108.0,
                bottom: 922.0,
                width: 360.0,
                height: 320.0,
            }
        );
        assert_eq!(
            screen.panel(2000.0, Some(600.0), 2000.0, Side::Right),
            Frame {
                left: 732.0,
                bottom: 58.0,
                width: 960.0,
                height: 600.0,
            }
        );
        assert_eq!(
            screen.panel(520.0, Some(20_000.0), 0.0, Side::Left).height,
            1184.0
        );
        let top_overflow = Frame {
            left: 0.0,
            bottom: 1100.0,
            width: 500.0,
            height: 400.0,
        };
        assert_eq!(
            screen.constrain_panel(top_overflow, Side::Right).bottom,
            842.0
        );
        assert_eq!(
            screen
                .constrain_panel(
                    Frame {
                        bottom: -900.0,
                        ..top_overflow
                    },
                    Side::Left
                )
                .bottom,
            58.0
        );
    }

    #[test]
    fn smaller_screens_clip_displayed_size_without_mutating_preferred_dimensions() {
        let screen = Frame {
            left: -300.0,
            bottom: -300.0,
            width: 300.0,
            height: 300.0,
        };
        for side in [Side::Left, Side::Right] {
            assert_eq!(
                screen.panel(680.0, Some(540.0), 60.0, side),
                Frame {
                    left: -292.0,
                    bottom: -292.0,
                    width: 284.0,
                    height: 284.0
                }
            );
        }
        let tiny = Frame {
            width: 10.0,
            height: 10.0,
            ..Frame::default()
        };
        let panel = tiny.panel(680.0, Some(540.0), 60.0, Side::Right);
        assert_eq!(panel.width, 1.0);
        assert_eq!(panel.height, 1.0);
        assert!(panel.left.is_finite() && panel.bottom.is_finite());
    }
    #[test]
    fn edge_requires_dwell_and_pointer_exit_before_reopening() {
        let now = Instant::now();
        let mut edge = EdgeTrigger::default();
        assert!(!edge.update(true, now));
        assert!(edge.update(true, now + Duration::from_millis(200)));
        assert!(!edge.update(true, now + Duration::from_secs(1)));
        edge.reset_until_leave();
        assert!(!edge.update(true, now + Duration::from_secs(2)));
        assert!(!edge.update(false, now + Duration::from_secs(3)));
        assert!(!edge.update(true, now + Duration::from_secs(4)));
        assert!(edge.update(true, now + Duration::from_millis(4200)));
    }
}
