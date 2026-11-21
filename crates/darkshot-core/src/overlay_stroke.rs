//! Nét đang vẽ của lớp phủ: bắt đầu, kéo, chốt và vùng cần vẽ lại; luật thuần dùng chung.

use crate::annotate::{
    add_point, blur_pixel_rect, constrain_45, constrain_square, dip_to_px, redact_color, too_short,
    Pt, Shape, SizeLevel, Tool,
};
use crate::geometry::Rect;
use crate::image::Image;
use crate::overlay_layout::union;
use crate::theme::Rgb;

/// Nét đang vẽ: công cụ, điểm đầu (tọa độ ảnh chụp) và hình hiện tại.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub tool: Tool,
    pub start: Pt,
    pub shape: Shape,
    /// Điểm chuột cuối (để nhấn hoặc nhả Shift cập nhật nét mà không cần di chuột).
    pub cursor: Pt,
}

/// Màu che ban đầu khi không lấy được điểm ảnh dưới chuột.
pub const BLUR_FALLBACK: Rgb = Rgb(128, 128, 128);

/// Màu của điểm ảnh chứa `p` trong ảnh `img`; `None` khi ngoài ảnh.
pub fn pixel_color(img: &Image, p: Pt) -> Option<Rgb> {
    let px = Rect {
        x: p.x.floor() as i32,
        y: p.y.floor() as i32,
        w: 1,
        h: 1,
    };
    redact_color(img, px)
}

/// Màu che cho vùng `a`–`b` tính trên ảnh chụp gốc `img`; `old` khi vùng nằm ngoài ảnh.
pub fn blur_color(img: &Image, a: Pt, b: Pt, old: Rgb) -> Rgb {
    redact_color(img, blur_pixel_rect(a, b)).unwrap_or(old)
}

/// Bắt đầu nét tại `p` với độ dày theo mức `level` và `dpi` của màn hình chứa `p`; `None` với
/// Chữ, Chọn, Đánh số (không có nét kéo).
/// `img`: ảnh chụp gốc (màu che của Làm mờ; màu `color` của bảng màu không dùng cho nó).
pub fn begin_stroke(
    tool: Tool,
    p: Pt,
    dpi: u32,
    level: SizeLevel,
    color: Rgb,
    img: &Image,
) -> Option<Stroke> {
    let dip = if tool == Tool::Marker {
        level.marker_dip()
    } else {
        level.stroke_dip()
    };
    let width = dip_to_px(dip, dpi);
    let shape = match tool {
        Tool::Pen => Shape::Pen {
            points: vec![p],
            width,
            color,
        },
        Tool::Marker => Shape::Marker {
            points: vec![p],
            width,
            color,
            opacity: crate::annotate::HIGHLIGHT_OPACITY,
        },
        Tool::Line => Shape::Line {
            a: p,
            b: p,
            width,
            color,
        },
        Tool::Arrow => Shape::Arrow {
            a: p,
            b: p,
            width,
            color,
        },
        Tool::Rect => Shape::Rect {
            a: p,
            b: p,
            width,
            color,
        },
        Tool::Ellipse => Shape::Ellipse {
            a: p,
            b: p,
            width,
            color,
        },
        Tool::Blur => Shape::Blur {
            a: p,
            b: p,
            color: pixel_color(img, p).unwrap_or(BLUR_FALLBACK),
        },
        Tool::Text | Tool::Select | Tool::Step => return None,
    };
    Some(Stroke {
        tool,
        start: p,
        shape,
        cursor: p,
    })
}

/// Hộp bao đoạn `p`–`q` nới `pad` mỗi phía, làm tròn ra ngoài.
pub fn segment_dirty(p: Pt, q: Pt, pad: f32) -> Rect {
    let x0 = (p.x.min(q.x) - pad).floor() as i32;
    let y0 = (p.y.min(q.y) - pad).floor() as i32;
    let x1 = (p.x.max(q.x) + pad).ceil() as i32;
    let y1 = (p.y.max(q.y) + pad).ceil() as i32;
    Rect {
        x: x0,
        y: y0,
        w: x1.saturating_sub(x0),
        h: y1.saturating_sub(y0),
    }
}

/// Kéo nét tới `p` (`shift`: khóa góc 45° hoặc hình vuông). Trả vùng cần vẽ lại, `None` khi không đổi.
/// Nét tự do chỉ làm mới hộp bao đoạn mới (nới nửa độ dày, đầu vuông của bút dạ quang thì nhân
/// căn 2, cộng 2 px khử răng cưa), không phụ thuộc độ dài nét.
/// Làm mờ: tính lại màu che trên ảnh gốc `img` mỗi khi `b` đổi.
pub fn extend_stroke(s: &mut Stroke, p: Pt, shift: bool, img: &Image) -> Option<Rect> {
    s.cursor = p;
    let marker = s.tool == Tool::Marker;
    match &mut s.shape {
        Shape::Pen { points, width, .. } | Shape::Marker { points, width, .. } => {
            let last = *points.last()?;
            let n = points.len();
            add_point(points, p);
            if points.len() == n {
                return None;
            }
            let half = *width / 2.0;
            let reach = if marker {
                half * std::f32::consts::SQRT_2
            } else {
                half
            };
            Some(segment_dirty(last, p, reach + 2.0))
        }
        _ => {
            let end = match (s.tool, shift) {
                (Tool::Line | Tool::Arrow, true) => constrain_45(s.start, p),
                (Tool::Rect | Tool::Ellipse | Tool::Blur, true) => constrain_square(s.start, p),
                _ => p,
            };
            let before = s.shape.bounds();
            match &mut s.shape {
                Shape::Line { b, .. }
                | Shape::Arrow { b, .. }
                | Shape::Rect { b, .. }
                | Shape::Ellipse { b, .. } => {
                    if *b == end {
                        return None;
                    }
                    *b = end;
                }
                Shape::Blur { a, b, color } => {
                    if *b == end {
                        return None;
                    }
                    *b = end;
                    *color = blur_color(img, *a, end, *color);
                }
                _ => return None,
            }
            union(before, s.shape.bounds())
        }
    }
}

/// Hình chốt từ nét vừa vẽ; `None` khi Đường thẳng, Mũi tên, Hình chữ nhật, Hình elip, Làm mờ
/// quá ngắn.
/// Làm mờ: màu che tính lại lần cuối trên ảnh gốc `img`.
pub fn end_stroke(s: Stroke, img: &Image) -> Option<Shape> {
    match s.shape {
        Shape::Line { a, b, .. }
        | Shape::Arrow { a, b, .. }
        | Shape::Rect { a, b, .. }
        | Shape::Ellipse { a, b, .. }
        | Shape::Blur { a, b, .. }
            if too_short(s.tool, a, b) =>
        {
            None
        }
        Shape::Blur { a, b, color } => Some(Shape::Blur {
            a,
            b,
            color: blur_color(img, a, b, color),
        }),
        shape => Some(shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_picker::{size_for_key, VK_OEM_4, VK_OEM_6};

    /// `[` / `]` đổi mức trong giới hạn; phím khác không đổi. Độ dày nét theo mức.
    #[test]
    fn size_keys() {
        use crate::annotate::DEFAULT_COLOR;
        let (dec, inc) = (VK_OEM_4, VK_OEM_6);
        assert_eq!(size_for_key(SizeLevel::Medium, dec), Some(SizeLevel::Small));
        assert_eq!(size_for_key(SizeLevel::Small, dec), Some(SizeLevel::Small));
        assert_eq!(size_for_key(SizeLevel::Medium, inc), Some(SizeLevel::Large));
        assert_eq!(size_for_key(SizeLevel::Large, inc), Some(SizeLevel::Huge));
        assert_eq!(size_for_key(SizeLevel::Huge, inc), Some(SizeLevel::Huge));
        assert_eq!(size_for_key(SizeLevel::Huge, dec), Some(SizeLevel::Large));
        assert_eq!(size_for_key(SizeLevel::Medium, u16::from(b'P')), None);
        let img = Image {
            width: 1,
            height: 1,
            bgra: vec![0, 0, 0, 255],
        };
        let p = Pt { x: 10.0, y: 10.0 };
        let width = |tool, dpi, level| match begin_stroke(tool, p, dpi, level, DEFAULT_COLOR, &img)
            .unwrap()
            .shape
        {
            Shape::Pen { width, .. }
            | Shape::Marker { width, .. }
            | Shape::Ellipse { width, .. } => width,
            other => panic!("{other:?}"),
        };
        assert_eq!(width(Tool::Pen, 96, SizeLevel::Large), 5.0);
        assert_eq!(width(Tool::Marker, 96, SizeLevel::Small), 10.0);
        // Mặc định (Vừa) đúng như hôm nay.
        assert_eq!(width(Tool::Pen, 144, SizeLevel::Medium), 4.5);
        assert_eq!(width(Tool::Marker, 96, SizeLevel::Medium), 16.0);
        assert_eq!(width(Tool::Ellipse, 192, SizeLevel::Huge), 16.0);
        // Hình elip: kéo, Shift thành hình tròn, quá ngắn thì bỏ.
        let mut s =
            begin_stroke(Tool::Ellipse, p, 96, SizeLevel::Medium, DEFAULT_COLOR, &img).unwrap();
        let q = Pt { x: 50.0, y: 30.0 };
        assert!(extend_stroke(&mut s, q, false, &img).is_some());
        assert!(matches!(s.shape, Shape::Ellipse { b, .. } if b == q));
        extend_stroke(&mut s, q, true, &img);
        assert!(matches!(s.shape, Shape::Ellipse { b, .. } if b == constrain_square(p, q)));
        assert!(matches!(end_stroke(s, &img), Some(Shape::Ellipse { .. })));
        let mut t =
            begin_stroke(Tool::Ellipse, p, 96, SizeLevel::Medium, DEFAULT_COLOR, &img).unwrap();
        extend_stroke(&mut t, Pt { x: 50.0, y: 11.0 }, false, &img);
        assert_eq!(end_stroke(t, &img), None);
        // Chữ, Chọn, Đánh số không có nét kéo.
        for tool in [Tool::Text, Tool::Select, Tool::Step] {
            assert!(begin_stroke(tool, p, 96, SizeLevel::Medium, DEFAULT_COLOR, &img).is_none());
        }
    }

    #[test]
    fn segment_invalidation_is_local() {
        use crate::annotate::{dip_to_px, DEFAULT_COLOR, STROKE_DIP};
        let dpi = 144;
        let width = dip_to_px(STROKE_DIP, dpi);
        let img = Image {
            width: 1,
            height: 1,
            bgra: vec![0, 0, 0, 255],
        };
        let mut s = begin_stroke(
            Tool::Pen,
            Pt { x: 100.0, y: 100.0 },
            dpi,
            SizeLevel::Medium,
            DEFAULT_COLOR,
            &img,
        )
        .unwrap();
        // Nét dài trước đó: hộp làm mới không phụ thuộc độ dài nét.
        for i in 1..=2000 {
            let p = Pt {
                x: 100.0 + i as f32 * 1.5,
                y: 100.0 + (i % 7) as f32,
            };
            assert!(extend_stroke(&mut s, p, false, &img).is_some());
        }
        let Shape::Pen { points, .. } = &s.shape else {
            panic!("không phải Pen");
        };
        let p = *points.last().unwrap();
        let q = Pt {
            x: p.x + 5.0,
            y: p.y - 3.0,
        };
        let r = extend_stroke(&mut s, q, false, &img).unwrap();
        let pad = width / 2.0 + 2.0;
        let (x0, x1) = (p.x.min(q.x) - pad, p.x.max(q.x) + pad);
        let (y0, y1) = (p.y.min(q.y) - pad, p.y.max(q.y) + pad);
        // Chứa hộp bao đoạn pq nới nửa độ dày + 2 px...
        assert!(r.x as f32 <= x0 && r.y as f32 <= y0, "{r:?}");
        assert!(r.right() as f32 >= x1 && r.bottom() as f32 >= y1, "{r:?}");
        // ...và không lớn hơn (chỉ làm tròn ra ngoài, dưới 1 px mỗi phía).
        assert!(r.x as f32 > x0 - 1.0 && r.y as f32 > y0 - 1.0, "{r:?}");
        assert!(
            (r.right() as f32) < x1 + 1.0 && (r.bottom() as f32) < y1 + 1.0,
            "{r:?}"
        );
        // Dịch dưới 1 px: không thêm điểm, không làm mới.
        let tiny = Pt {
            x: q.x + 0.4,
            y: q.y,
        };
        assert_eq!(extend_stroke(&mut s, tiny, false, &img), None);
    }

    /// Nét Làm mờ: màu ban đầu là điểm ảnh dưới chuột, mỗi lần kéo tính lại bằng `redact_color`,
    /// Shift thành hình vuông, màu bảng màu không dùng, quá ngắn thì bỏ.
    #[test]
    fn blur_stroke_tracks_redact_color() {
        use crate::annotate::{blur_pixel_rect, redact_color, DEFAULT_COLOR};
        let (w, h) = (40u32, 30u32);
        let mut bgra = Vec::new();
        for y in 0..h {
            for x in 0..w {
                bgra.extend_from_slice(&[(x * 6) as u8, (y * 8) as u8, ((x + y) * 3) as u8, 255]);
            }
        }
        let img = Image {
            width: w,
            height: h,
            bgra,
        };
        let color_of = |s: &Stroke| match s.shape {
            Shape::Blur { color, .. } => color,
            _ => panic!("không phải Blur"),
        };
        let a = Pt { x: 5.0, y: 4.0 };
        let mut s =
            begin_stroke(Tool::Blur, a, 144, SizeLevel::Medium, DEFAULT_COLOR, &img).unwrap();
        // Điểm ảnh (5, 4): B = 30, G = 32, R = 27.
        assert_eq!(color_of(&s), Rgb(27, 32, 30));
        let b = Pt { x: 20.5, y: 12.2 };
        let r = extend_stroke(&mut s, b, false, &img).unwrap();
        let want = redact_color(&img, blur_pixel_rect(a, b));
        assert_eq!(Some(color_of(&s)), want);
        // Vùng làm mới chứa cả vùng che mới.
        let bb = s.shape.bounds().unwrap();
        assert!(r.x <= bb.x && r.y <= bb.y && r.right() >= bb.right() && r.bottom() >= bb.bottom());
        // Shift: hình vuông.
        extend_stroke(&mut s, b, true, &img).unwrap();
        let Shape::Blur { b: sq, .. } = s.shape else {
            panic!()
        };
        assert_eq!(sq, constrain_square(a, b));
        assert_eq!(
            Some(color_of(&s)),
            redact_color(&img, blur_pixel_rect(a, sq))
        );
        // Kéo ra ngoài ảnh: màu tính trên phần trong ảnh, không panic.
        let far = Pt {
            x: 500.0,
            y: -300.0,
        };
        extend_stroke(&mut s, far, false, &img);
        assert_eq!(
            Some(color_of(&s)),
            redact_color(&img, blur_pixel_rect(a, far))
        );
        // Chốt: màu tính lại trên hình chữ nhật cuối.
        let fin = Pt { x: 30.0, y: 25.0 };
        extend_stroke(&mut s, fin, false, &img);
        let mut done = s.clone();
        if let Shape::Blur { color, .. } = &mut done.shape {
            *color = Rgb(1, 2, 3);
        }
        assert_eq!(
            end_stroke(done, &img),
            Some(Shape::Blur {
                a,
                b: fin,
                color: redact_color(&img, blur_pixel_rect(a, fin)).unwrap(),
            })
        );
        // Quá ngắn: bỏ.
        let mut t =
            begin_stroke(Tool::Blur, a, 96, SizeLevel::Medium, DEFAULT_COLOR, &img).unwrap();
        extend_stroke(&mut t, Pt { x: 30.0, y: 5.0 }, false, &img);
        assert_eq!(end_stroke(t, &img), None);
        // Điểm đầu ngoài ảnh: xám.
        let out = begin_stroke(
            Tool::Blur,
            Pt { x: -9.0, y: 3.0 },
            96,
            SizeLevel::Medium,
            DEFAULT_COLOR,
            &img,
        )
        .unwrap();
        assert_eq!(color_of(&out), Rgb(128, 128, 128));
    }
}
