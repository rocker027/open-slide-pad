use crate::model::Side;
use std::time::{Duration, Instant};

pub const MIN_WIDTH: f64 = 360.0;
pub const MAX_WIDTH: f64 = 960.0;
pub const MIN_HEIGHT: f64 = 320.0;
pub const MAX_HEIGHT: f64 = 10_000.0;
pub const MARGIN: f64 = 8.0;

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
        if now.duration_since(start) >= Duration::from_millis(180) {
            self.latched = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
