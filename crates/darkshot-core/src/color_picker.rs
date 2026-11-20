//! Bảng chọn màu của lớp phủ: màu đang sửa, ô số, hit-test và ảnh nền; luật thuần dùng chung.

use crate::annotate::SizeLevel;
use crate::color::{
    hls_to_rgb, lum_at, rgb_to_hls, spectrum_hs, Hls, PickerLayout, HLS_MAX, HUE_MAX, RGB_MAX,
};
use crate::geometry::{Point, Rect};
use crate::overlay_layout::{scaled, LUM_ARROW};
use crate::theme::Rgb;

/// Mã phím ảo Windows của `[` (`VK_OEM_4`) và `]` (`VK_OEM_6`); là số, không phụ thuộc hệ điều hành.
pub const VK_OEM_4: u16 = 0xDB;
pub const VK_OEM_6: u16 = 0xDD;

/// `[` (`VK_OEM_4`) giảm, `]` (`VK_OEM_6`) tăng mức cỡ; phím khác `None`.
pub fn size_for_key(level: SizeLevel, vk: u16) -> Option<SizeLevel> {
    match vk {
        VK_OEM_4 => Some(level.smaller()),
        VK_OEM_6 => Some(level.larger()),
        _ => None,
    }
}

/// Đường kính chấm của mức cỡ trên lưới biểu tượng 24.
pub fn size_dot(level: SizeLevel) -> f32 {
    match level {
        SizeLevel::Small => 6.0,
        SizeLevel::Medium => 9.0,
        SizeLevel::Large => 12.0,
        SizeLevel::Huge => 15.0,
    }
}

/// Màu hiện tại của phần tự chọn: giữ cả HLS và RGB (như hộp Windows) để sửa một ô số không làm
/// trôi các ô cùng hệ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickColor {
    pub hls: Hls,
    pub rgb: Rgb,
}

impl PickColor {
    pub fn from_rgb(rgb: Rgb) -> PickColor {
        PickColor {
            hls: rgb_to_hls(rgb),
            rgb,
        }
    }

    pub fn from_hls(hls: Hls) -> PickColor {
        let hls = Hls {
            h: hls.h.min(HUE_MAX),
            l: hls.l.min(HLS_MAX),
            s: hls.s.min(HLS_MAX),
        };
        PickColor {
            hls,
            rgb: hls_to_rgb(hls),
        }
    }
}

/// Giá trị lớn nhất của sáu ô số: Hue, Sat, Lum, Red, Green, Blue.
pub const FIELD_MAX: [u16; 6] = [HUE_MAX, HLS_MAX, HLS_MAX, RGB_MAX, RGB_MAX, RGB_MAX];
pub const FIELD_COUNT: usize = 6;
/// Số chữ số tối đa của một ô số.
pub const FIELD_DIGITS: usize = 3;

/// Giá trị ô số `i` của màu `c`.
pub fn field_value(c: PickColor, i: usize) -> u16 {
    match i {
        0 => c.hls.h,
        1 => c.hls.s,
        2 => c.hls.l,
        3 => u16::from(c.rgb.0),
        4 => u16::from(c.rgb.1),
        _ => u16::from(c.rgb.2),
    }
}

/// Màu `c` sau khi ô `i` nhận giá trị `v` (kẹp vào khoảng của ô). Ô HLS thì RGB tính lại từ HLS;
/// ô RGB thì HLS tính lại từ RGB.
pub fn with_field(c: PickColor, i: usize, v: u16) -> PickColor {
    let v = v.min(FIELD_MAX.get(i).copied().unwrap_or(0));
    // Sau khi kẹp, ô RGB luôn vừa u8.
    let b = u8::try_from(v).unwrap_or(u8::MAX);
    match i {
        0 => PickColor::from_hls(Hls { h: v, ..c.hls }),
        1 => PickColor::from_hls(Hls { s: v, ..c.hls }),
        2 => PickColor::from_hls(Hls { l: v, ..c.hls }),
        3 => PickColor::from_rgb(Rgb(b, c.rgb.1, c.rgb.2)),
        4 => PickColor::from_rgb(Rgb(c.rgb.0, b, c.rgb.2)),
        5 => PickColor::from_rgb(Rgb(c.rgb.0, c.rgb.1, b)),
        _ => c,
    }
}

/// Ô số đang sửa: chỉ số ô, chữ đang gõ; `fresh`: chưa gõ gì (chữ số đầu tiên thay cả giá trị).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldEdit {
    pub idx: usize,
    pub text: String,
    pub fresh: bool,
}

impl FieldEdit {
    pub fn new(idx: usize, c: PickColor) -> FieldEdit {
        FieldEdit {
            idx,
            text: field_value(c, idx).to_string(),
            fresh: true,
        }
    }

    pub fn digit(&mut self, d: char) {
        if !d.is_ascii_digit() {
            return;
        }
        if self.fresh {
            self.text.clear();
            self.fresh = false;
        }
        if self.text.len() < FIELD_DIGITS {
            self.text.push(d);
        }
    }

    pub fn backspace(&mut self) {
        self.fresh = false;
        self.text.pop();
    }

    /// Màu sau khi chốt chữ đang gõ (kẹp); chữ rỗng thì giữ nguyên.
    pub fn commit(&self, c: PickColor) -> PickColor {
        match self.text.parse::<u16>() {
            Ok(v) => with_field(c, self.idx, v),
            Err(_) => c,
        }
    }

    /// Lên (`up`) hoặc xuống: chốt chữ đang gõ, tăng/giảm 1 (kẹp biên), chữ hiện giá trị mới.
    pub fn step(&mut self, c: PickColor, up: bool) -> PickColor {
        let c = self.commit(c);
        let v = field_value(c, self.idx);
        let v = if up {
            v.saturating_add(1)
        } else {
            v.saturating_sub(1)
        };
        let c = with_field(c, self.idx, v);
        *self = FieldEdit::new(self.idx, c);
        c
    }
}

/// Phần của bảng chọn màu dưới chuột.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerHit {
    Basic(usize),
    Custom(usize),
    Define,
    Spectrum,
    Lum,
    Field(usize),
    Add,
    Ok,
    Cancel,
    /// Trong bảng nhưng không trúng phần nào.
    Panel,
}

/// Khóa của ảnh thanh độ sáng: Hue, Sat, rộng, cao.
pub type LumKey = (u16, u16, i32, i32);

/// Đang kéo trong ô sắc độ hoặc thanh độ sáng.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerDrag {
    Spectrum,
    Lum,
}

/// Dời `r` (tương đối góc bảng) theo góc trên trái của bảng `pal`.
pub fn at(pal: Rect, r: Rect) -> Rect {
    Rect {
        x: pal.x + r.x,
        y: pal.y + r.y,
        ..r
    }
}

/// Vùng bấm của thanh độ sáng (tương đối): thanh và cột tam giác bên phải nó.
pub fn lum_hit_rect(l: &PickerLayout, dpi: u32) -> Rect {
    let right = l.lum_arrow_x + scaled(LUM_ARROW, dpi) + scaled(2, dpi);
    Rect {
        w: (right - l.lum_bar.x).max(l.lum_bar.w),
        ..l.lum_bar
    }
}

/// Phần dưới điểm `p` của bảng `pal` có bố cục `l`; ngoài bảng trả `None`.
pub fn picker_hit(l: &PickerLayout, dpi: u32, pal: Rect, p: Point) -> Option<PickerHit> {
    let panel = Rect {
        x: pal.x,
        y: pal.y,
        w: l.size.0,
        h: l.size.1,
    };
    if !panel.contains(p) {
        return None;
    }
    let inside = |r: Rect| r.w > 0 && r.h > 0 && at(pal, r).contains(p);
    if let Some(i) = l.basic.iter().position(|r| inside(*r)) {
        return Some(PickerHit::Basic(i));
    }
    if let Some(i) = l.custom.iter().position(|r| inside(*r)) {
        return Some(PickerHit::Custom(i));
    }
    let single = [
        (l.define_btn, PickerHit::Define),
        (l.spectrum, PickerHit::Spectrum),
        (lum_hit_rect(l, dpi), PickerHit::Lum),
        (l.add_btn, PickerHit::Add),
        (l.ok_btn, PickerHit::Ok),
        (l.cancel_btn, PickerHit::Cancel),
    ];
    if let Some((_, h)) = single.iter().find(|(r, _)| inside(*r)) {
        return Some(*h);
    }
    if let Some(i) = l.fields.iter().position(|(_, b)| inside(*b)) {
        return Some(PickerHit::Field(i));
    }
    Some(PickerHit::Panel)
}

/// Ảnh BGRA `w`×`h` của ô sắc độ: Hue theo chiều ngang, Sat theo chiều dọc, Lum 120.
pub fn spectrum_image(w: i32, h: i32) -> Vec<u8> {
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    let rect = Rect { x: 0, y: 0, w, h };
    let mut bits = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let (hue, sat) = spectrum_hs(rect, x, y);
            let c = hls_to_rgb(Hls {
                h: hue,
                l: HLS_MAX / 2,
                s: sat,
            });
            bits.extend_from_slice(&[c.2, c.1, c.0, 0xFF]);
        }
    }
    bits
}

/// Ảnh BGRA `w`×`h` của thanh độ sáng cho Hue `hue`, Sat `sat`: Lum 240 ở trên, 0 ở dưới.
pub fn lum_image(hue: u16, sat: u16, w: i32, h: i32) -> Vec<u8> {
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    let bar = Rect { x: 0, y: 0, w, h };
    let mut bits = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        let c = hls_to_rgb(Hls {
            h: hue,
            l: lum_at(bar, y),
            s: sat,
        });
        for _ in 0..w {
            bits.extend_from_slice(&[c.2, c.1, c.0, 0xFF]);
        }
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::picker_layout;

    #[test]
    fn picker_hit_testing() {
        let dpi = 144;
        let pal = Rect {
            x: 200,
            y: 300,
            w: 0,
            h: 0,
        };
        let center = |r: Rect| Point {
            x: pal.x + r.x + r.w / 2,
            y: pal.y + r.y + r.h / 2,
        };
        for expanded in [false, true] {
            let l = picker_layout(dpi, expanded);
            let pal = Rect {
                w: l.size.0,
                h: l.size.1,
                ..pal
            };
            let hit = |p| picker_hit(&l, dpi, pal, p);
            for (i, r) in l.basic.iter().enumerate() {
                assert_eq!(hit(center(*r)), Some(PickerHit::Basic(i)), "ô {i}");
            }
            for (i, r) in l.custom.iter().enumerate() {
                assert_eq!(hit(center(*r)), Some(PickerHit::Custom(i)), "ô {i}");
            }
            assert_eq!(hit(center(l.define_btn)), Some(PickerHit::Define));
            // Khe giữa hai ô, lề, nhãn: trong bảng nhưng không trúng phần nào.
            let gap = Point {
                x: pal.x + l.basic[0].right() + 1,
                y: center(l.basic[0]).y,
            };
            assert_eq!(hit(gap), Some(PickerHit::Panel));
            assert_eq!(hit(center(l.basic_label)), Some(PickerHit::Panel));
            assert_eq!(hit(Point { x: 201, y: 301 }), Some(PickerHit::Panel));
            // Ngoài bảng.
            assert_eq!(hit(Point { x: 199, y: 310 }), None);
            assert_eq!(
                hit(Point {
                    x: pal.right(),
                    y: 310
                }),
                None
            );
            assert_eq!(
                hit(Point {
                    x: 210,
                    y: pal.bottom()
                }),
                None
            );
            if !expanded {
                continue;
            }
            assert_eq!(hit(center(l.spectrum)), Some(PickerHit::Spectrum));
            assert_eq!(hit(center(l.lum_bar)), Some(PickerHit::Lum));
            // Cột tam giác bên phải thanh cũng là thanh độ sáng.
            let arrow = Point {
                x: pal.x + l.lum_arrow_x + scaled(LUM_ARROW, dpi) / 2,
                y: center(l.lum_bar).y,
            };
            assert_eq!(hit(arrow), Some(PickerHit::Lum));
            for (i, (label, bx)) in l.fields.iter().enumerate() {
                assert_eq!(hit(center(*bx)), Some(PickerHit::Field(i)), "ô số {i}");
                assert_eq!(hit(center(*label)), Some(PickerHit::Panel), "nhãn {i}");
            }
            assert_eq!(hit(center(l.preview)), Some(PickerHit::Panel));
            assert_eq!(hit(center(l.add_btn)), Some(PickerHit::Add));
            assert_eq!(hit(center(l.ok_btn)), Some(PickerHit::Ok));
            assert_eq!(hit(center(l.cancel_btn)), Some(PickerHit::Cancel));
        }
    }

    #[test]
    fn field_edit_rules() {
        let c = PickColor::from_rgb(Rgb(10, 20, 30));
        assert_eq!(c.hls, rgb_to_hls(Rgb(10, 20, 30)));
        for (i, v) in [(3, 10), (4, 20), (5, 30)] {
            assert_eq!(field_value(c, i), v);
        }
        assert_eq!(field_value(c, 0), c.hls.h);
        assert_eq!(field_value(c, 1), c.hls.s);
        assert_eq!(field_value(c, 2), c.hls.l);
        // Gõ 2, 5, 9 vào Red: kẹp 255 khi chốt; Hue/Sat/Lum theo rgb_to_hls.
        let mut e = FieldEdit::new(3, c);
        assert_eq!(e.text, "10");
        for d in ['2', '5', '9'] {
            e.digit(d);
        }
        assert_eq!(e.text, "259");
        // Tối đa 3 chữ số.
        e.digit('1');
        assert_eq!(e.text, "259");
        let red = e.commit(c);
        assert_eq!(red.rgb, Rgb(255, 20, 30));
        assert_eq!(red.hls, rgb_to_hls(Rgb(255, 20, 30)));
        // Hue "300" -> 239; RGB theo hls_to_rgb.
        let mut e = FieldEdit::new(0, c);
        for d in ['3', '0', '0'] {
            e.digit(d);
        }
        let hue = e.commit(c);
        assert_eq!(hue.hls.h, 239);
        assert_eq!((hue.hls.s, hue.hls.l), (c.hls.s, c.hls.l));
        assert_eq!(hue.rgb, hls_to_rgb(hue.hls));
        // Sat, Lum kẹp 240.
        let mut e = FieldEdit::new(1, c);
        for d in ['9', '9', '9'] {
            e.digit(d);
        }
        assert_eq!(e.commit(c).hls.s, 240);
        // Backspace: "25" -> "2" -> "20".
        let mut e = FieldEdit::new(4, c);
        e.digit('2');
        e.digit('5');
        e.backspace();
        assert_eq!(e.text, "2");
        e.digit('0');
        assert_eq!(e.commit(c).rgb, Rgb(10, 20, 30));
        let mut e = FieldEdit::new(4, c);
        e.digit('2');
        e.digit('5');
        e.backspace();
        e.digit('1');
        assert_eq!(e.commit(c).rgb, Rgb(10, 21, 30));
        // Backspace ngay khi mới chọn ô: bớt chữ số cuối của giá trị.
        let mut e = FieldEdit::new(3, c);
        e.backspace();
        assert_eq!(e.text, "1");
        assert_eq!(e.commit(c).rgb, Rgb(1, 20, 30));
        // Xóa hết rồi chốt: giữ nguyên.
        e.backspace();
        assert_eq!(e.text, "");
        assert_eq!(e.commit(c), c);
        // Lên/xuống: tăng/giảm 1, kẹp biên, chữ hiện giá trị mới.
        let mut e = FieldEdit::new(3, c);
        let up = e.step(c, true);
        assert_eq!(up.rgb, Rgb(11, 20, 30));
        assert_eq!(e.text, "11");
        let white = PickColor::from_rgb(Rgb(255, 255, 255));
        let mut e = FieldEdit::new(3, white);
        assert_eq!(e.step(white, true), white);
        assert_eq!(e.text, "255");
        let black = PickColor::from_rgb(Rgb(0, 0, 0));
        let mut e = FieldEdit::new(5, black);
        assert_eq!(e.step(black, false), black);
        let top = PickColor::from_hls(Hls {
            h: 239,
            l: 240,
            s: 0,
        });
        let mut e = FieldEdit::new(0, top);
        assert_eq!(e.step(top, true).hls.h, 239);
        let mut e = FieldEdit::new(2, top);
        assert_eq!(e.step(top, true).hls.l, 240);
        let mut e = FieldEdit::new(1, top);
        assert_eq!(e.step(top, false).hls.s, 0);
        // Lên chốt chữ đang gõ trước rồi mới tăng.
        let mut e = FieldEdit::new(3, c);
        e.digit('4');
        e.digit('0');
        assert_eq!(e.step(c, true).rgb, Rgb(41, 20, 30));
        assert_eq!(e.text, "41");
        // with_field: HLS tính lại RGB, RGB tính lại HLS.
        let g = with_field(c, 5, 999);
        assert_eq!(g.rgb, Rgb(10, 20, 255));
        assert_eq!(g.hls, rgb_to_hls(g.rgb));
        let l = with_field(c, 2, 200);
        assert_eq!(l.hls, Hls { l: 200, ..c.hls });
        assert_eq!(l.rgb, hls_to_rgb(l.hls));
        assert_eq!(PickColor::from_hls(l.hls), l);
    }

    #[test]
    fn picker_images() {
        let (w, h) = (50, 40);
        let sp = spectrum_image(w, h);
        assert_eq!(sp.len(), (w * h * 4) as usize);
        let px = |bits: &[u8], w: i32, x: i32, y: i32| {
            let i = ((y * w + x) * 4) as usize;
            Rgb(bits[i + 2], bits[i + 1], bits[i])
        };
        // Góc trên trái: Hue 0, Sat 240, Lum 120 = đỏ; hàng dưới cùng: xám Lum 120.
        assert_eq!(px(&sp, w, 0, 0), Rgb(255, 0, 0));
        let grey = hls_to_rgb(Hls { h: 0, l: 120, s: 0 });
        assert_eq!(px(&sp, w, 0, h - 1), grey);
        assert_eq!(px(&sp, w, w - 1, h - 1), grey);
        let rect = Rect { x: 0, y: 0, w, h };
        let (hh, ss) = spectrum_hs(rect, 30, 10);
        assert_eq!(
            px(&sp, w, 30, 10),
            hls_to_rgb(Hls {
                h: hh,
                l: 120,
                s: ss
            })
        );
        // Thanh độ sáng: trên trắng, dưới đen, giữa theo lum_at.
        let lum = lum_image(0, 240, 3, 60);
        assert_eq!(lum.len(), 3 * 60 * 4);
        assert_eq!(px(&lum, 3, 0, 0), Rgb(255, 255, 255));
        assert_eq!(px(&lum, 3, 2, 59), Rgb(0, 0, 0));
        let bar = Rect {
            x: 0,
            y: 0,
            w: 3,
            h: 60,
        };
        assert_eq!(
            px(&lum, 3, 1, 25),
            hls_to_rgb(Hls {
                h: 0,
                l: lum_at(bar, 25),
                s: 240
            })
        );
        assert!(spectrum_image(0, 10).is_empty());
    }
}
