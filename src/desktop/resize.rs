use super::{App, native};
use anyhow::Result;
use sliderust::{
    model::Settings,
    panel::{Frame, MARGIN, MAX_HEIGHT, MAX_WIDTH, MIN_HEIGHT, MIN_WIDTH},
};
use tao::dpi::LogicalSize;

pub(super) struct PointerResize {
    frame: Frame,
    pointer: (f64, f64),
}

impl App {
    pub(super) fn resize_limits(&self) {
        let width = (self.screen.width - 2.0 * MARGIN).clamp(1.0, MAX_WIDTH);
        let height = (self.screen.height - 2.0 * MARGIN).clamp(1.0, MAX_HEIGHT);
        self.window.set_min_inner_size(Some(LogicalSize::new(
            MIN_WIDTH.min(width),
            MIN_HEIGHT.min(height),
        )));
        self.window
            .set_max_inner_size(Some(LogicalSize::new(width, height)));
    }

    fn begin_resize(&mut self) {
        self.resize_origin.get_or_insert(self.frame);
        self.animation = None;
        self.auto_hide.pause();
        native::opacity(&self.window, 1.0);
    }

    pub(super) fn begin_pointer_resize(&mut self) {
        let pointer = native::mouse();
        let frame = native::window_frame(&self.window);
        let left = match self.settings.side {
            sliderust::model::Side::Right => frame.left,
            sliderust::model::Side::Left => frame.left + frame.width - 24.0,
        };
        let grip = Frame {
            left,
            bottom: frame.bottom,
            width: 24.0,
            height: 24.0,
        };
        if !self.visible || !native::left_mouse_down() || !grip.contains(pointer) {
            return;
        }
        self.begin_resize();
        self.pointer_resize = Some(PointerResize { frame, pointer });
    }

    pub(super) fn window_resized(&mut self) -> Result<()> {
        let actual = native::window_frame(&self.window);
        let changed = (actual.width - self.frame.width).abs() > 0.5
            || (actual.height - self.frame.height).abs() > 0.5;
        // 自己 setFrame 前已更新 frame；只接收原生互動或系統造成的新尺寸。
        if self.visible
            && self.pointer_resize.is_none()
            && (changed || native::is_live_resize(&self.window))
        {
            self.begin_resize();
            self.frame = actual;
        }
        self.layout()
    }

    pub(super) fn poll_resize(&mut self) -> Result<bool> {
        if let Some(drag) = &self.pointer_resize {
            if native::left_mouse_down() {
                let pointer = native::mouse();
                let dx = pointer.0 - drag.pointer.0;
                let dy = pointer.1 - drag.pointer.1;
                let width = drag.frame.width
                    + if self.settings.side == sliderust::model::Side::Right {
                        -dx
                    } else {
                        dx
                    };
                let height = drag.frame.height - dy;
                let proposed = Frame {
                    width,
                    height,
                    bottom: drag.frame.bottom + dy,
                    ..drag.frame
                };
                let frame = self.screen.constrain_panel(proposed, self.settings.side);
                if frame != self.frame {
                    self.frame = frame;
                    native::set_frame(&self.window, frame);
                    self.layout()?;
                }
                return Ok(true);
            }
            self.pointer_resize = None;
        }
        if native::is_live_resize(&self.window) {
            self.window_resized()?;
            return Ok(true);
        }
        if self.resize_origin.is_some() {
            self.finish_resize()?;
        }
        Ok(false)
    }

    pub(super) fn finish_resize(&mut self) -> Result<()> {
        let Some(previous) = self.resize_origin.take() else {
            return Ok(());
        };
        let actual = native::window_frame(&self.window);
        let target = self.screen.constrain_panel(actual, self.settings.side);
        if target == previous {
            self.frame = previous;
            return self.layout();
        }
        let updated = Settings {
            width: target.width.max(MIN_WIDTH),
            height: if (target.height - previous.height).abs() > 0.5 {
                Some(target.height.max(MIN_HEIGHT))
            } else {
                self.settings.height
            },
            top_offset: (self.screen.bottom + self.screen.height - target.bottom - target.height)
                .max(MARGIN),
            ..self.settings.clone()
        };
        if let Err(error) = self.commit(updated) {
            self.frame = previous;
            native::set_frame(&self.window, previous);
            self.layout()?;
            return Err(error.context("無法保存視窗大小，已恢復原本尺寸"));
        }
        self.frame = target;
        native::set_frame(&self.window, target);
        self.layout()?;
        self.render()
    }
}
