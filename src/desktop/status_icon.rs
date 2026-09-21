//! 選單列圖示：與介面左上角相同的「邊緣滑出面板」符號。
//!
//! 以距離函式直接畫成 RGBA，不為一張 18 pt 的圖示引入影像解碼相依套件。
//! 圖示是 template image：系統只看 alpha，顏色由選單列的明暗外觀決定，所以 RGB 一律為 0。

/// 選單列圖示高 18 pt；以 2x 繪製，Retina 螢幕不模糊。
pub const SIZE: u32 = 36;

/// 介面符號的座標系（`ui/index.html` 的 viewBox 為 32）。
const VIEW_BOX: f64 = 32.0;
const FRAME: RoundedRect = RoundedRect {
    left: 4.0,
    top: 5.0,
    width: 20.0,
    height: 22.0,
    radius: 4.0,
};
const FRAME_STROKE: f64 = 3.0;
const PANEL: RoundedRect = RoundedRect {
    left: 13.0,
    top: 5.0,
    width: 15.0,
    height: 22.0,
    radius: 4.0,
};
/// 面板上的把手：template image 沒有第二種顏色，改以鏤空表示。
const HANDLE: RoundedRect = RoundedRect {
    left: 16.0,
    top: 13.0,
    width: 2.0,
    height: 6.0,
    radius: 1.0,
};

struct RoundedRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    radius: f64,
}

impl RoundedRect {
    /// 點到圓角矩形邊界的有號距離：內部為負、外部為正。
    fn distance(&self, x: f64, y: f64) -> f64 {
        let (half_width, half_height) = (self.width / 2.0, self.height / 2.0);
        let beyond_x = (x - self.left - half_width).abs() - (half_width - self.radius);
        let beyond_y = (y - self.top - half_height).abs() - (half_height - self.radius);
        beyond_x.max(0.0).hypot(beyond_y.max(0.0)) + beyond_x.max(beyond_y).min(0.0) - self.radius
    }
}

/// 距離邊界一個像素寬的範圍內線性過渡，作為反鋸齒。
fn coverage(distance: f64, pixel: f64) -> f64 {
    (0.5 - distance / pixel).clamp(0.0, 1.0)
}

/// 以列為主的 RGBA 像素，邊長 [`SIZE`]。
pub fn rgba() -> Vec<u8> {
    let pixel = VIEW_BOX / f64::from(SIZE);
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for row in 0..SIZE {
        for column in 0..SIZE {
            let (x, y) = (
                (f64::from(column) + 0.5) * pixel,
                (f64::from(row) + 0.5) * pixel,
            );
            let frame = coverage(FRAME.distance(x, y).abs() - FRAME_STROKE / 2.0, pixel);
            let panel = coverage(PANEL.distance(x, y), pixel);
            let handle = coverage(HANDLE.distance(x, y), pixel);
            let alpha = frame.max(panel) * (1.0 - handle);
            pixels.extend([0, 0, 0, (alpha * 255.0).round() as u8]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(pixels: &[u8], x: f64, y: f64) -> u8 {
        let scale = f64::from(SIZE) / VIEW_BOX;
        let (column, row) = ((x * scale) as usize, (y * scale) as usize);
        pixels[(row * SIZE as usize + column) * 4 + 3]
    }

    #[test]
    fn icon_is_a_template_image_of_the_sliding_panel_mark() {
        let pixels = rgba();
        assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
        assert!(
            pixels.chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]),
            "template image 只用 alpha 表示形狀"
        );
        // 四個角落與圖示外圍透明。
        for (x, y) in [
            (0.5, 0.5),
            (31.5, 0.5),
            (0.5, 31.5),
            (31.5, 31.5),
            (1.0, 16.0),
        ] {
            assert_eq!(alpha_at(&pixels, x, y), 0, "({x}, {y})");
        }
        // 外框的左邊線、右側實心面板不透明。
        assert_eq!(alpha_at(&pixels, 4.0, 16.0), 255, "外框左邊線");
        assert_eq!(alpha_at(&pixels, 24.0, 16.0), 255, "面板內部");
        // 外框內的凹槽與面板上的把手是鏤空的。
        assert_eq!(alpha_at(&pixels, 9.0, 16.0), 0, "外框內側");
        assert_eq!(alpha_at(&pixels, 17.0, 16.0), 0, "把手");
        // 圓角邊緣有反鋸齒，不是只有 0 與 255。
        assert!(pixels.chunks(4).any(|pixel| (1..255).contains(&pixel[3])));
    }
}
