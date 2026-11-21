//! Đổi tọa độ giữa hệ Cocoa (điểm, gốc dưới trái màn hình chính, Y hướng lên) và hệ của core
//! (pixel, gốc trên trái, Y hướng xuống). Thuần, không gọi Objective-C.

use darkshot_core::geometry::Rect;

/// Hình chữ nhật số thực (đơn vị điểm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectF {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Khung màn hình Cocoa (gốc dưới trái) sang toạ độ toàn cục gốc trên trái, đơn vị điểm.
pub fn cocoa_to_top_left(frame: RectF, primary_height: f64) -> RectF {
    // Cạnh trên của khung trong Cocoa là `y + h`; trục Y lật quanh cạnh trên màn hình chính.
    RectF {
        x: frame.x,
        y: primary_height - (frame.y + frame.h),
        w: frame.w,
        h: frame.h,
    }
}

/// Điểm trong view (đơn vị điểm, gốc trên trái của màn hình đó) sang pixel của ảnh màn hình đó.
pub fn pt_to_px(v: f64, scale: f64) -> i32 {
    // `as` bão hòa ở biên i32 và đổi NaN thành 0, không panic.
    (v * valid_scale(scale)).round() as i32
}

/// Vùng chọn (điểm) sang vùng pixel của ảnh `img_w`×`img_h`: gốc làm tròn xuống, cạnh xa làm
/// tròn lên, kẹp vào ảnh; rỗng thì `None`.
pub fn selection_to_pixels(sel: RectF, scale: f64, img_w: i32, img_h: i32) -> Option<Rect> {
    let finite = [sel.x, sel.y, sel.w, sel.h, scale]
        .iter()
        .all(|v| v.is_finite());
    if !finite || scale <= 0.0 || sel.w <= 0.0 || sel.h <= 0.0 || img_w <= 0 || img_h <= 0 {
        return None;
    }
    // Sai số dấu phẩy động (ví dụ 7.000000000000001) không được thêm hay bớt một pixel.
    // `as i64` bão hòa nên giá trị rất lớn vẫn an toàn trước khi kẹp.
    let lo = |v: f64, max: i32| ((v * scale + EDGE_EPS).floor() as i64).clamp(0, i64::from(max));
    let hi = |v: f64, max: i32| ((v * scale - EDGE_EPS).ceil() as i64).clamp(0, i64::from(max));
    let (x0, x1) = (lo(sel.x, img_w), hi(sel.x + sel.w, img_w));
    let (y0, y1) = (lo(sel.y, img_h), hi(sel.y + sel.h, img_h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // Mọi giá trị đã kẹp trong [0, img_w] hoặc [0, img_h] nên vừa i32.
    Some(Rect {
        x: x0 as i32,
        y: y0 as i32,
        w: (x1 - x0) as i32,
        h: (y1 - y0) as i32,
    })
}

/// Pixel sang điểm.
pub fn px_to_pt(v: i32, scale: f64) -> f64 {
    f64::from(v) / valid_scale(scale)
}

/// Dung sai khi làm tròn biên vùng chọn (pixel).
const EDGE_EPS: f64 = 1e-6;

/// Hệ số hợp lệ (dương, hữu hạn); ngược lại coi như 1x.
fn valid_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> RectF {
        RectF { x, y, w, h }
    }

    fn px(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn primary_screen_maps_to_origin() {
        assert_eq!(
            cocoa_to_top_left(r(0.0, 0.0, 1920.0, 1080.0), 1080.0),
            r(0.0, 0.0, 1920.0, 1080.0)
        );
    }

    #[test]
    fn secondary_above_primary_has_negative_top() {
        // Cocoa: màn hình phụ nằm trên có y = chiều cao màn hình chính.
        assert_eq!(
            cocoa_to_top_left(r(0.0, 1080.0, 2560.0, 1440.0), 1080.0),
            r(0.0, -1440.0, 2560.0, 1440.0)
        );
    }

    #[test]
    fn secondary_below_primary_has_negative_cocoa_origin() {
        assert_eq!(
            cocoa_to_top_left(r(100.0, -900.0, 1440.0, 900.0), 1080.0),
            r(100.0, 1080.0, 1440.0, 900.0)
        );
    }

    #[test]
    fn secondary_left_of_primary_keeps_negative_x() {
        // Màn hình trái thấp hơn, đáy thẳng hàng với màn hình chính.
        assert_eq!(
            cocoa_to_top_left(r(-1440.0, 0.0, 1440.0, 900.0), 1080.0),
            r(-1440.0, 180.0, 1440.0, 900.0)
        );
    }

    #[test]
    fn primary_height_changes_the_flip() {
        assert_eq!(
            cocoa_to_top_left(r(0.0, 900.0, 1920.0, 1080.0), 900.0),
            r(0.0, -1080.0, 1920.0, 1080.0)
        );
        assert_eq!(
            cocoa_to_top_left(r(1512.0, 0.0, 1920.0, 1080.0), 982.0),
            r(1512.0, -98.0, 1920.0, 1080.0)
        );
    }

    #[test]
    fn retina_2x_selection() {
        assert_eq!(
            selection_to_pixels(r(10.0, 20.0, 100.0, 50.0), 2.0, 3024, 1964),
            Some(px(20, 40, 200, 100))
        );
    }

    #[test]
    fn standard_1x_selection() {
        assert_eq!(
            selection_to_pixels(r(10.0, 20.0, 100.0, 50.0), 1.0, 1920, 1080),
            Some(px(10, 20, 100, 50))
        );
    }

    #[test]
    fn fractional_edges_keep_far_pixel() {
        // 1x: x từ 10.25 đến 110.75 chạm pixel 10..=110 (101 pixel).
        assert_eq!(
            selection_to_pixels(r(10.25, 0.0, 100.5, 10.0), 1.0, 1920, 1080),
            Some(px(10, 0, 101, 10))
        );
        // 2x: 20.5 .. 221.5 chạm pixel 20..=221 (202 pixel).
        assert_eq!(
            selection_to_pixels(r(10.25, 0.0, 100.5, 10.0), 2.0, 3840, 2160),
            Some(px(20, 0, 202, 20))
        );
    }

    #[test]
    fn float_noise_does_not_add_a_pixel() {
        // 0.07 * 100 = 7.000000000000001: cạnh xa không được làm tròn lên thành 8.
        assert_eq!(
            selection_to_pixels(r(0.0, 0.0, 0.07, 0.01), 100.0, 1000, 1000),
            Some(px(0, 0, 7, 1))
        );
        // 0.57 * 100 = 56.99999999999999: gốc không được làm tròn xuống thành 56.
        assert_eq!(
            selection_to_pixels(r(0.57, 0.0, 0.43, 0.01), 100.0, 1000, 1000),
            Some(px(57, 0, 43, 1))
        );
        // Biên đúng nguyên sau khi nhân: 1/3 điểm ở 3x là đúng 1 pixel.
        assert_eq!(
            selection_to_pixels(r(1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0), 3.0, 10, 10),
            Some(px(1, 0, 1, 3))
        );
    }

    #[test]
    fn clamps_to_image_edges() {
        assert_eq!(
            selection_to_pixels(r(-5.0, -5.0, 20.0, 20.0), 2.0, 100, 100),
            Some(px(0, 0, 30, 30))
        );
        assert_eq!(
            selection_to_pixels(r(90.0, 95.0, 20.0, 20.0), 1.0, 100, 100),
            Some(px(90, 95, 10, 5))
        );
        // Kéo đúng tới mép phải/dưới của màn hình Retina: không dư pixel.
        assert_eq!(
            selection_to_pixels(r(0.0, 0.0, 1512.0, 982.0), 2.0, 3024, 1964),
            Some(px(0, 0, 3024, 1964))
        );
    }

    #[test]
    fn empty_negative_or_outside_is_none() {
        assert_eq!(
            selection_to_pixels(r(10.0, 10.0, 0.0, 5.0), 2.0, 100, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(10.0, 10.0, 5.0, 0.0), 2.0, 100, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(10.0, 10.0, -5.0, 5.0), 2.0, 100, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(10.0, 10.0, 5.0, -5.0), 2.0, 100, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(200.0, 10.0, 5.0, 5.0), 1.0, 100, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(-20.0, 10.0, 5.0, 5.0), 1.0, 100, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(f64::NAN, 10.0, 5.0, 5.0), 1.0, 100, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(1.0, 1.0, 5.0, 5.0), 1.0, 0, 100),
            None
        );
        assert_eq!(
            selection_to_pixels(r(1.0, 1.0, 5.0, 5.0), 0.0, 100, 100),
            None
        );
    }

    #[test]
    fn pt_to_px_rounds_to_nearest() {
        assert_eq!(pt_to_px(10.0, 2.0), 20);
        assert_eq!(pt_to_px(10.2, 1.0), 10);
        assert_eq!(pt_to_px(10.25, 2.0), 21);
        assert_eq!(pt_to_px(-3.0, 2.0), -6);
        assert_eq!(pt_to_px(f64::NAN, 2.0), 0);
    }

    #[test]
    fn px_pt_round_trip() {
        for scale in [1.0, 2.0, 1.5, 3.0] {
            for v in [-1440, -1, 0, 1, 7, 1511, 3024] {
                assert_eq!(pt_to_px(px_to_pt(v, scale), scale), v, "{v} @ {scale}");
            }
        }
        assert_eq!(px_to_pt(20, 2.0), 10.0);
        assert_eq!(px_to_pt(7, 1.0), 7.0);
    }
}
