//! Vẽ các hình đánh dấu bằng Core Graphics và CoreText (Task 110): một hàm vẽ dùng chung cho lớp phủ
//! và cho ảnh ra (`burn`), tương đương `darkshot-windows/src/annotate_draw.rs` (GDI+).

use darkshot_core::annotate::{Shape, TextEdit};
use darkshot_core::geometry::Rect;
use darkshot_core::image::Image;
use objc2_core_graphics::CGContext;

pub fn draw_shapes(_ctx: &CGContext, _shapes: &[Shape], _ox: i32, _oy: i32, _clip: Rect) -> bool {
    false
}

pub fn shape_bounds(_s: &Shape) -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    }
}

pub fn caret_rect(_edit: &TextEdit) -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    }
}

fn with_bitmap(_img: &mut Image, _paint: impl FnOnce(&CGContext) -> bool) -> Result<(), String> {
    Err("todo".into())
}

pub fn burn(_img: &mut Image, _shapes: &[Shape], _crop: Rect) -> Result<(), String> {
    Err("todo".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::annotate::{blur_pixel_rect, outline_color, Pt, TEXT_DIP};
    use darkshot_core::theme::Rgb;

    const RED: Rgb = Rgb(255, 0, 0);
    const BLUE: Rgb = Rgb(0, 0, 255);
    const YELLOW: Rgb = Rgb(255, 255, 0);

    fn pt(x: f32, y: f32) -> Pt {
        Pt { x, y }
    }

    fn solid(w: u32, h: u32, v: u8) -> Image {
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            bgra.extend_from_slice(&[v, v, v, 255]);
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    fn noise(w: u32, h: u32, seed: u32) -> Image {
        let mut s = seed;
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            for _ in 0..3 {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                bgra.push((s >> 24) as u8);
            }
            bgra.push(255);
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    /// [B, G, R, A] tại (x, y).
    fn px(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [
            img.bgra[i],
            img.bgra[i + 1],
            img.bgra[i + 2],
            img.bgra[i + 3],
        ]
    }

    fn bgr(c: Rgb) -> [u8; 4] {
        [c.2, c.1, c.0, 255]
    }

    fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| x.abs_diff(*y) <= tol)
    }

    fn full(img: &Image) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: img.width as i32,
            h: img.height as i32,
        }
    }

    fn all_opaque(img: &Image) -> bool {
        img.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 255)
    }

    /// Vẽ `shapes` lên ảnh nền xám `bg` cỡ w×h (crop = cả ảnh) và trả ảnh.
    fn render(w: u32, h: u32, bg: u8, shapes: &[Shape]) -> Image {
        let mut img = solid(w, h, bg);
        let crop = full(&img);
        assert_eq!(burn(&mut img, shapes, crop), Ok(()));
        assert!(all_opaque(&img));
        img
    }

    fn blur_at(x: f32, y: f32, w: f32, h: f32, c: Rgb) -> Shape {
        Shape::Blur {
            a: pt(x, y),
            b: pt(x + w, y + h),
            color: c,
        }
    }

    fn text(origin: Pt, t: &str, size: f32, color: Rgb) -> Shape {
        Shape::Text {
            origin,
            text: t.into(),
            size,
            color,
        }
    }

    #[test]
    fn empty_burn_is_identity() {
        let mut img = noise(64, 40, 7);
        let before = img.clone();
        let crop = Rect {
            x: 13,
            y: 9,
            w: 64,
            h: 40,
        };
        assert_eq!(burn(&mut img, &[], crop), Ok(()));
        assert_eq!(img, before);
    }

    #[test]
    fn rect_outline_not_filled() {
        let r = Shape::Rect {
            a: pt(50.0, 40.0),
            b: pt(10.0, 10.0),
            width: 4.0,
            color: RED,
        };
        let img = render(60, 50, 0, &[r]);
        assert_eq!(px(&img, 30, 25), [0, 0, 0, 255], "tâm là nền");
        assert_eq!(px(&img, 10, 25), bgr(RED), "cạnh trái");
        assert_eq!(px(&img, 30, 10), bgr(RED), "cạnh trên");
        assert_eq!(px(&img, 49, 25), bgr(RED), "cạnh phải");
        assert_eq!(px(&img, 30, 5), [0, 0, 0, 255], "ngoài khung");
    }

    #[test]
    fn line_and_pen() {
        let line = Shape::Line {
            a: pt(5.0, 10.0),
            b: pt(35.0, 10.0),
            width: 4.0,
            color: RED,
        };
        let img = render(40, 20, 0, &[line]);
        assert_eq!(px(&img, 20, 10), bgr(RED));
        assert_eq!(px(&img, 20, 15), [0, 0, 0, 255]);
        // Đầu tròn: nhô ra ngoài hai đầu nửa độ dày.
        assert!(px(&img, 4, 10)[2] > 128, "đầu tròn trái");

        let pen = Shape::Pen {
            points: vec![pt(5.0, 5.0), pt(5.0, 35.0), pt(35.0, 35.0)],
            width: 4.0,
            color: BLUE,
        };
        let img = render(40, 40, 0, &[pen]);
        assert!(near(px(&img, 5, 20), bgr(BLUE), 2));
        assert!(near(px(&img, 20, 35), bgr(BLUE), 2));
        assert_eq!(px(&img, 20, 20), [0, 0, 0, 255]);

        // Mọi điểm trùng: một chấm tròn; rỗng điểm: không vẽ gì.
        let dot = Shape::Pen {
            points: vec![pt(20.0, 20.0); 3],
            width: 10.0,
            color: RED,
        };
        let img = render(40, 40, 0, &[dot]);
        assert_eq!(px(&img, 20, 20), bgr(RED));
        assert_eq!(px(&img, 20, 30), [0, 0, 0, 255]);
        let none = Shape::Pen {
            points: vec![],
            width: 10.0,
            color: RED,
        };
        let marker_none = Shape::Marker {
            points: vec![],
            width: 10.0,
            color: RED,
        };
        let img = render(20, 20, 7, &[none, marker_none]);
        assert_eq!(img, solid(20, 20, 7));
    }

    #[test]
    fn arrow_head_wider_than_shaft() {
        let a = Shape::Arrow {
            a: pt(10.0, 30.0),
            b: pt(90.0, 30.0),
            width: 4.0,
            color: RED,
        };
        let img = render(100, 60, 0, &[a]);
        assert_eq!(px(&img, 40, 30), bgr(RED), "thân");
        assert_eq!(px(&img, 40, 34), [0, 0, 0, 255], "ngoài thân");
        // Đầu: dài max(4w, 12 * w / 3) = 16, rộng ±16*sin28 ≈ 7,5 ở gốc đầu.
        assert!(
            px(&img, 80, 36)[2] > 200,
            "đầu rộng hơn thân: {:?}",
            px(&img, 80, 36)
        );
        assert!(px(&img, 80, 24)[2] > 200);
        assert_eq!(px(&img, 95, 30), [0, 0, 0, 255], "sau mũi tên");
    }

    #[test]
    fn ellipse_outline() {
        let e = Shape::Ellipse {
            a: pt(10.0, 10.0),
            b: pt(70.0, 50.0),
            width: 4.0,
            color: RED,
        };
        let img = render(80, 60, 0, &[e]);
        assert!(near(px(&img, 10, 30), bgr(RED), 2), "mép trái");
        assert!(near(px(&img, 40, 10), bgr(RED), 2), "mép trên");
        assert!(near(px(&img, 69, 30), bgr(RED), 2), "mép phải");
        assert_eq!(px(&img, 40, 30), [0, 0, 0, 255], "tâm rỗng");
        assert_eq!(px(&img, 12, 12), [0, 0, 0, 255], "góc ngoài elip");
    }

    #[test]
    fn marker_is_translucent_and_single_coverage() {
        let m = Shape::Marker {
            points: vec![pt(10.0, 20.0), pt(50.0, 20.0)],
            width: 10.0,
            color: RED,
        };
        let img = render(60, 40, 100, &[m]);
        // 0,4 * 255 + 0,6 * 100 = 162; 0,6 * 100 = 60.
        let p = px(&img, 30, 20);
        assert!(near(p, [60, 60, 162, 255], 3), "{p:?}");
        assert_eq!(px(&img, 30, 30), [100, 100, 100, 255]);
        // Đầu vuông: nhô ra nửa độ dày.
        assert!(near(px(&img, 6, 20), [60, 60, 162, 255], 3));
        // Nét tự cắt chỉ phủ một lần (không đậm hơn chỗ khác).
        let x = Shape::Marker {
            points: vec![
                pt(10.0, 10.0),
                pt(50.0, 30.0),
                pt(50.0, 10.0),
                pt(10.0, 30.0),
            ],
            width: 10.0,
            color: RED,
        };
        let img = render(60, 40, 100, &[x]);
        let cross = px(&img, 30, 20);
        assert!(
            near(cross, [60, 60, 162, 255], 3),
            "giao điểm không đậm hơn: {cross:?}"
        );
    }

    #[test]
    fn blur_covers_exact_pixel_rect() {
        let b = Shape::Blur {
            a: pt(10.2, 10.7),
            b: pt(30.5, 20.1),
            color: BLUE,
        };
        let r = blur_pixel_rect(pt(10.2, 10.7), pt(30.5, 20.1));
        assert_eq!((r.x, r.y, r.w, r.h), (10, 10, 21, 11));
        let img = render(50, 40, 0, &[b]);
        for (x, y, inside) in [
            (10, 10, true),
            (30, 20, true),
            (20, 15, true),
            (9, 15, false),
            (31, 15, false),
            (20, 9, false),
            (20, 21, false),
        ] {
            let want = if inside { bgr(BLUE) } else { [0, 0, 0, 255] };
            assert_eq!(px(&img, x, y), want, "({x},{y})");
        }
    }

    #[test]
    fn step_has_disc_ring_and_number() {
        let s = Shape::Step {
            center: pt(40.0, 40.0),
            radius: 20.0,
            n: 1,
            color: RED,
        };
        let img = render(80, 80, 255, &[s]);
        assert_eq!(px(&img, 40 + 12, 40), bgr(RED), "lòng màu");
        let ink = outline_color(RED);
        assert!(
            near(px(&img, 40 + 19, 40), [ink.2, ink.1, ink.0, 255], 2),
            "viền"
        );
        assert_eq!(px(&img, 40 + 23, 40), [255, 255, 255, 255], "ngoài vòng");
        // Có số: ít nhất một điểm trong lòng gần màu mực (đen).
        let mut inked = 0;
        for y in 30..50 {
            for x in 32..48 {
                if near(px(&img, x, y), [0, 0, 0, 255], 40) {
                    inked += 1;
                }
            }
        }
        assert!(inked >= 10, "số 1 phải có nét: {inked}");
        // n = 0: không số.
        let s0 = Shape::Step {
            center: pt(40.0, 40.0),
            radius: 20.0,
            n: 0,
            color: RED,
        };
        let img0 = render(80, 80, 255, &[s0]);
        assert_eq!(px(&img0, 40, 40), bgr(RED));
        // Số nhiều chữ số không tràn vòng: cỡ chữ thu lại.
        let s99 = Shape::Step {
            center: pt(40.0, 40.0),
            radius: 20.0,
            n: 9999,
            color: RED,
        };
        let img = render(80, 80, 255, &[s99]);
        assert_eq!(px(&img, 40 + 23, 40), [255, 255, 255, 255]);
    }

    #[test]
    fn text_bounds_grow_with_content() {
        let o = pt(10.0, 10.0);
        let b = |t: &str, size: f32| shape_bounds(&text(o, t, size, YELLOW));
        let short = b("ab", 24.0);
        let long = b("abcdefgh", 24.0);
        assert!(long.w > short.w, "{short:?} {long:?}");
        let small = b("abcd", 16.0);
        let big = b("abcd", 48.0);
        assert!(big.w > small.w && big.h > small.h);
        let two = b("ab\ncd", 24.0);
        assert!(two.h > short.h, "nhiều dòng cao hơn: {short:?} {two:?}");
        assert!(short.w > 0 && short.h > 0);
        // Hộp chứa gốc đặt chữ.
        assert!(short.x <= 10 && short.y <= 10);
        // Hình khác Text: dùng Shape::bounds của core.
        let l = Shape::Line {
            a: pt(0.0, 0.0),
            b: pt(10.0, 0.0),
            width: 2.0,
            color: RED,
        };
        assert_eq!(Some(shape_bounds(&l)), l.bounds());
        // Danh sách điểm rỗng: hộp rỗng.
        let e = Shape::Pen {
            points: vec![],
            width: 2.0,
            color: RED,
        };
        assert_eq!(shape_bounds(&e).w, 0);
    }

    #[test]
    fn text_draws_fill_and_outline_inside_its_bounds() {
        let t = text(pt(20.0, 20.0), "HI", 80.0, YELLOW);
        let bounds = shape_bounds(&t);
        let img = render(240, 160, 128, &[t]);
        let (mut yellow, mut black, mut outside_changed) = (0, 0, 0);
        for y in 0..img.height {
            for x in 0..img.width {
                let p = px(&img, x, y);
                if p == bgr(YELLOW) {
                    yellow += 1;
                }
                if near(p, [0, 0, 0, 255], 8) {
                    black += 1;
                }
                let inside = (x as i32) >= bounds.x
                    && (x as i32) < bounds.x + bounds.w
                    && (y as i32) >= bounds.y
                    && (y as i32) < bounds.y + bounds.h;
                if !inside && p != [128, 128, 128, 255] {
                    outside_changed += 1;
                }
            }
        }
        assert!(yellow > 200, "ruột chữ màu: {yellow}");
        assert!(black > 100, "viền ngoài khác màu chữ: {black}");
        assert_eq!(
            outside_changed, 0,
            "mọi điểm đã vẽ nằm trong hộp bao {bounds:?}"
        );
        // Viền dày 2 × TEXT_OUTLINE_DIP × size / TEXT_DIP / 2 ở mỗi phía ngoài.
        let _ = TEXT_DIP;
    }

    #[test]
    fn text_unicode_empty_and_long_do_not_panic() {
        let o = pt(5.0, 5.0);
        for s in [
            "",
            "\n\n",
            "   ",
            "Việt Nam ạ ơ ư",
            "😀 emoji 👍🏽",
            "漢字かな",
            &"w".repeat(3000),
        ] {
            let t = text(o, s, 20.0, RED);
            let b = shape_bounds(&t);
            assert!(b.w >= 0 && b.h >= 0, "{s:?}: {b:?}");
            let _ = render(60, 40, 0, &[t]);
        }
        let blank = render(30, 30, 9, &[text(o, "", 20.0, RED)]);
        assert_eq!(blank, solid(30, 30, 9), "chuỗi rỗng không vẽ gì");
        let vi = shape_bounds(&text(o, "Việt", 24.0, RED));
        assert!(vi.w > 20 && vi.h > 10);
    }

    #[test]
    fn caret_follows_caret_position() {
        let o = pt(10.0, 20.0);
        let mut e = TextEdit::new(o, 20.0, YELLOW);
        let c0 = caret_rect(&e);
        assert_eq!((c0.x, c0.y), (10, 20));
        assert_eq!(c0.w, ((20.0f32 / TEXT_DIP).round() as i32).max(1));
        assert!(c0.h >= 20, "cao ít nhất một em: {c0:?}");
        e.insert("ab");
        let c2 = caret_rect(&e);
        e.insert("c");
        let c3 = caret_rect(&e);
        assert!(c2.x > c0.x && c3.x > c2.x, "{c0:?} {c2:?} {c3:?}");
        assert_eq!(c2.y, 20);
        e.insert("\nx");
        let c4 = caret_rect(&e);
        assert!(c4.y >= 20 + c0.h - 1, "xuống dòng: {c4:?}");
        assert!(c4.x > c0.x && c4.x < c3.x);
        // Con trỏ ở giữa chuỗi.
        e.left();
        e.left();
        let c5 = caret_rect(&e);
        assert_eq!(c5.x, c3.x, "đầu dòng 2 ở 'x' ... (đầu dòng 1 sau 'c')");
    }

    // ------------------------------------------------------------ z-order (yêu cầu của chủ dự án)

    #[test]
    fn z_order_later_rect_is_in_front() {
        let red = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(50.0, 50.0),
            width: 10.0,
            color: RED,
        };
        let blue = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(50.0, 50.0),
            width: 10.0,
            color: BLUE,
        };
        let img = render(60, 60, 0, &[red.clone(), blue.clone()]);
        assert_eq!(px(&img, 10, 30), bgr(BLUE), "thêm sau nằm trước");
        let img = render(60, 60, 0, &[blue, red]);
        assert_eq!(px(&img, 10, 30), bgr(RED), "đảo thứ tự thì đảo kết quả");
    }

    /// Mỗi loại hình phủ đặc điểm `probe` bằng màu `color`.
    fn covering(kind: usize, color: Rgb) -> (Shape, (u32, u32)) {
        match kind {
            0 => (
                Shape::Pen {
                    points: vec![pt(10.0, 40.0), pt(90.0, 40.0)],
                    width: 20.0,
                    color,
                },
                (50, 40),
            ),
            1 => (
                Shape::Line {
                    a: pt(10.0, 40.0),
                    b: pt(90.0, 40.0),
                    width: 20.0,
                    color,
                },
                (50, 40),
            ),
            2 => (
                Shape::Arrow {
                    a: pt(10.0, 40.0),
                    b: pt(100.0, 40.0),
                    width: 20.0,
                    color,
                },
                (50, 40),
            ),
            3 => (
                Shape::Rect {
                    a: pt(10.0, 10.0),
                    b: pt(90.0, 70.0),
                    width: 20.0,
                    color,
                },
                (10, 40),
            ),
            4 => (
                Shape::Ellipse {
                    a: pt(10.0, 10.0),
                    b: pt(90.0, 70.0),
                    width: 20.0,
                    color,
                },
                (10, 40),
            ),
            5 => (
                Shape::Step {
                    center: pt(50.0, 40.0),
                    radius: 30.0,
                    n: 0,
                    color,
                },
                (50, 40),
            ),
            _ => (blur_at(30.0, 20.0, 40.0, 40.0, color), (50, 40)),
        }
    }

    #[test]
    fn z_order_every_kind_against_blur() {
        for kind in 0..7 {
            let (shape, (x, y)) = covering(kind, RED);
            // Blur xanh phủ đặc điểm dò.
            let cover = blur_at(x as f32 - 4.0, y as f32 - 4.0, 8.0, 8.0, BLUE);
            let img = render(100, 80, 200, &[shape.clone(), cover.clone()]);
            assert_eq!(
                px(&img, x, y),
                bgr(BLUE),
                "loại {kind}: Blur sau che hình trước"
            );
            let img = render(100, 80, 200, &[cover, shape]);
            assert!(
                near(px(&img, x, y), bgr(RED), 2),
                "loại {kind}: hình sau nằm trước Blur"
            );
        }
    }

    #[test]
    fn z_order_marker_blends_only_with_what_is_below() {
        let blue = blue_block();
        let marker = Shape::Marker {
            points: vec![pt(10.0, 20.0), pt(50.0, 20.0)],
            width: 10.0,
            color: RED,
        };
        // Marker sau: pha với xanh bên dưới (không còn là xanh thuần, không thành đỏ thuần).
        let img = render(60, 40, 100, &[blue.clone(), marker.clone()]);
        let p = px(&img, 30, 20);
        // 0,4 * (0,0,255) đỏ + 0,6 * xanh: B = 0,6 * 255 = 153, R = 102.
        assert!(near(p, [153, 0, 102, 255], 3), "{p:?}");
        // Marker trước, khối xanh sau: xanh thuần che hết.
        let img = render(60, 40, 100, &[marker, blue]);
        assert_eq!(px(&img, 30, 20), bgr(BLUE));
    }

    fn blue_block() -> Shape {
        blur_at(5.0, 10.0, 50.0, 20.0, BLUE)
    }

    #[test]
    fn z_order_text_vs_shapes_by_index() {
        // Tìm một điểm ruột chữ.
        let t = text(pt(20.0, 20.0), "H", 100.0, YELLOW);
        let alone = render(200, 200, 128, std::slice::from_ref(&t));
        let (mut found, mut cnt) = (None, 0);
        'f: for y in 0..alone.height {
            for x in 0..alone.width {
                if px(&alone, x, y) == bgr(YELLOW) {
                    // Cần cả lân cận trong ruột (ổn định trước cạnh khung/khối che).
                    cnt += 1;
                    if cnt > 400 {
                        found = Some((x, y));
                        break 'f;
                    }
                }
            }
        }
        let (px_, py_) = found.expect("phải có điểm ruột chữ");
        let (fx, fy) = (px_ as f32, py_ as f32);
        // Khối đặc (Blur) trước chữ: chữ nằm trên. Sau chữ: bị che.
        let cover = blur_at(fx - 3.0, fy - 3.0, 6.0, 6.0, BLUE);
        let img = render(200, 200, 128, &[cover.clone(), t.clone()]);
        assert_eq!(
            px(&img, px_, py_),
            bgr(YELLOW),
            "chữ thêm sau nằm trên khối"
        );
        let img = render(200, 200, 128, &[t.clone(), cover]);
        assert_eq!(px(&img, px_, py_), bgr(BLUE), "chữ thêm trước bị khối che");
        // Khung (Rect) dày đi qua điểm đó.
        let frame = Shape::Rect {
            a: pt(fx, fy - 50.0),
            b: pt(fx + 80.0, fy + 50.0),
            width: 20.0,
            color: RED,
        };
        let img = render(200, 200, 128, &[frame.clone(), t.clone()]);
        assert_eq!(px(&img, px_, py_), bgr(YELLOW), "chữ sau nằm trên khung");
        let img = render(200, 200, 128, &[t, frame]);
        assert_eq!(px(&img, px_, py_), bgr(RED), "khung sau nằm trên chữ");
    }

    #[test]
    fn z_order_follows_index_not_kind() {
        // Rect, Text, Rect: Rect thứ hai nằm trên cả chữ lẫn Rect đầu; thứ tự không gom theo loại.
        let r1 = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(90.0, 70.0),
            width: 20.0,
            color: RED,
        };
        let r2 = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(90.0, 70.0),
            width: 20.0,
            color: BLUE,
        };
        let t = text(pt(12.0, 30.0), "HHH", 40.0, YELLOW);
        let img = render(100, 80, 0, &[r1, t, r2]);
        assert_eq!(px(&img, 10, 40), bgr(BLUE));
        assert_eq!(px(&img, 89, 40), bgr(BLUE));
    }

    // ------------------------------------------------------------ burn

    #[test]
    fn burn_offsets_clips_and_stays_opaque() {
        // Ảnh đã cắt từ vùng (100, 50): hình ở tọa độ ảnh chụp.
        let mut img = solid(40, 30, 0);
        let crop = Rect {
            x: 100,
            y: 50,
            w: 40,
            h: 30,
        };
        let r = Shape::Rect {
            a: pt(110.0, 60.0),
            b: pt(130.0, 75.0),
            width: 4.0,
            color: RED,
        };
        assert_eq!(burn(&mut img, &[r], crop), Ok(()));
        assert_eq!(px(&img, 10, 12), bgr(RED), "cạnh trái ở (110-100, ..)");
        assert_eq!(px(&img, 5, 12), [0, 0, 0, 255]);
        assert!(all_opaque(&img));
        // Hình tràn mép ảnh và ngoài ảnh.
        let mut img = solid(40, 30, 0);
        let crop = full(&img);
        let l = Shape::Line {
            a: pt(-50.0, 15.0),
            b: pt(500.0, 15.0),
            width: 4.0,
            color: RED,
        };
        let far = Shape::Rect {
            a: pt(1000.0, 1000.0),
            b: pt(2000.0, 2000.0),
            width: 4.0,
            color: BLUE,
        };
        assert_eq!(burn(&mut img, &[l, far], crop), Ok(()));
        assert_eq!(px(&img, 0, 15), bgr(RED));
        assert_eq!(px(&img, 39, 15), bgr(RED));
        assert_eq!(px(&img, 20, 5), [0, 0, 0, 255]);
        // Vùng cắt nhỏ hơn ảnh: chỉ chạm trong vùng cắt.
        let mut img = solid(60, 40, 0);
        let crop = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 20,
        };
        let b = blur_at(0.0, 0.0, 60.0, 40.0, BLUE);
        assert_eq!(burn(&mut img, &[b], crop), Ok(()));
        assert_eq!(px(&img, 10, 10), bgr(BLUE));
        assert_eq!(px(&img, 30, 10), [0, 0, 0, 255], "ngoài vùng cắt");
        assert_eq!(px(&img, 10, 30), [0, 0, 0, 255]);
    }

    #[test]
    fn burn_rejects_invalid_image_and_survives_hostile_numbers() {
        let mut bad = Image {
            width: 4,
            height: 4,
            bgra: vec![0; 3],
        };
        let s = Shape::Line {
            a: pt(0.0, 0.0),
            b: pt(3.0, 3.0),
            width: 2.0,
            color: RED,
        };
        assert!(burn(
            &mut bad,
            &[s],
            Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 4
            }
        )
        .is_err());
        assert_eq!(bad.bgra, vec![0; 3], "ảnh không đổi khi lỗi");

        let nan = f32::NAN;
        let inf = f32::INFINITY;
        let hostile = vec![
            Shape::Line {
                a: pt(nan, 0.0),
                b: pt(5.0, 5.0),
                width: 2.0,
                color: RED,
            },
            Shape::Line {
                a: pt(0.0, 0.0),
                b: pt(inf, 5.0),
                width: 2.0,
                color: RED,
            },
            Shape::Pen {
                points: vec![pt(1e30, 1e30), pt(-1e30, 3.0)],
                width: 4.0,
                color: RED,
            },
            Shape::Rect {
                a: pt(0.0, 0.0),
                b: pt(5.0, 5.0),
                width: nan,
                color: RED,
            },
            Shape::Ellipse {
                a: pt(-inf, 0.0),
                b: pt(5.0, inf),
                width: 2.0,
                color: RED,
            },
            Shape::Arrow {
                a: pt(nan, nan),
                b: pt(nan, nan),
                width: 2.0,
                color: RED,
            },
            Shape::Arrow {
                a: pt(3.0, 3.0),
                b: pt(3.0, 3.0),
                width: 2.0,
                color: RED,
            },
            Shape::Marker {
                points: vec![pt(inf, inf)],
                width: 4.0,
                color: RED,
            },
            Shape::Blur {
                a: pt(nan, 1.0),
                b: pt(inf, 1e30),
                color: RED,
            },
            text(pt(nan, 1.0), "x", 20.0, RED),
            text(pt(1.0, 1.0), "x", f32::NAN, RED),
            text(pt(1.0, 1.0), "x", -5.0, RED),
            text(pt(1.0, 1.0), "x", 1e30, RED),
            Shape::Step {
                center: pt(nan, 1.0),
                radius: 5.0,
                n: 3,
                color: RED,
            },
            Shape::Step {
                center: pt(5.0, 5.0),
                radius: -3.0,
                n: 3,
                color: RED,
            },
            Shape::Step {
                center: pt(5.0, 5.0),
                radius: inf,
                n: 3,
                color: RED,
            },
        ];
        let mut img = solid(20, 20, 5);
        let crop = full(&img);
        let _ = burn(&mut img, &hostile, crop);
        assert!(all_opaque(&img));
        for s in &hostile {
            let _ = shape_bounds(s);
        }
    }

    #[test]
    fn burn_is_draw_shapes_in_list_order() {
        // `burn` và `draw_shapes` trên cùng ngữ cảnh cho cùng ảnh, nên cùng thứ tự lớp.
        let shapes = vec![
            blur_at(5.0, 5.0, 40.0, 30.0, BLUE),
            Shape::Marker {
                points: vec![pt(10.0, 20.0), pt(40.0, 20.0)],
                width: 12.0,
                color: RED,
            },
            text(pt(8.0, 8.0), "Ab", 24.0, YELLOW),
        ];
        let a = render(60, 50, 90, &shapes);
        let mut b = solid(60, 50, 90);
        let crop = full(&b);
        assert!(with_bitmap(&mut b, |c| draw_shapes(c, &shapes, 0, 0, crop)).is_ok());
        assert_eq!(a, b);
        // Đảo danh sách thì kết quả khác.
        let mut rev = shapes.clone();
        rev.reverse();
        assert_ne!(render(60, 50, 90, &rev), a);
    }
}
