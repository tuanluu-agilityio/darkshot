//! Lịch sử ảnh chụp: mô hình, chỉ mục JSON khoan dung, tìm/lọc, định dạng hiển thị.
//! Thuần lõi: không I/O.

use crate::config::Expiry;
use crate::i18n::{t, Lang, Msg};
use crate::naming::LocalTime;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const INDEX_VERSION: u32 = 1;
pub const MAX_ENTRIES: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Saved,
    Uploaded,
    Copied,
    UploadFallback,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Saved => "saved",
            Kind::Uploaded => "uploaded",
            Kind::Copied => "copied",
            Kind::UploadFallback => "upload_fallback",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Kind> {
        match s {
            "saved" => Some(Kind::Saved),
            "uploaded" => Some(Kind::Uploaded),
            "copied" => Some(Kind::Copied),
            "upload_fallback" => Some(Kind::UploadFallback),
            _ => None,
        }
    }

    pub fn label(self, lang: Lang) -> &'static str {
        t(
            lang,
            match self {
                Kind::Saved => Msg::HistoryKindSaved,
                Kind::Uploaded => Msg::HistoryKindUploaded,
                Kind::Copied => Msg::HistoryKindCopied,
                Kind::UploadFallback => Msg::HistoryKindFallback,
            },
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Saved,
    Uploaded,
    Copied,
}

impl Filter {
    pub fn accepts(self, k: Kind) -> bool {
        match self {
            Filter::All => true,
            Filter::Saved => matches!(k, Kind::Saved | Kind::UploadFallback),
            Filter::Uploaded => k == Kind::Uploaded,
            Filter::Copied => k == Kind::Copied,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub unix: u64,
    pub local: LocalTime,
    pub width: u32,
    pub height: u32,
    pub bytes: Option<u64>,
    pub path: Option<PathBuf>,
    pub managed: bool,
    pub url: Option<String>,
    pub expires: Option<String>,
}

impl Entry {
    pub fn file_name(&self) -> Option<String> {
        self.path
            .as_ref()?
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }

    pub fn image_path(&self, root: &Path) -> Option<PathBuf> {
        if self.managed {
            Some(root.join("images").join(format!("{}.png", self.id)))
        } else {
            self.path.clone()
        }
    }

    pub fn thumb_path(&self, root: &Path) -> PathBuf {
        root.join("thumbs").join(format!("{}.png", self.id))
    }
}

pub fn is_valid_id(id: &str) -> bool {
    id.len() == 21
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 13 hex của mili giây UNIX + 8 hex ngẫu nhiên.
///
/// Panic nếu nguồn ngẫu nhiên của hệ điều hành lỗi.
pub fn new_id(unix_millis: u64) -> String {
    let mut r = [0u8; 4];
    getrandom::fill(&mut r).expect("OS random source unavailable");
    format!(
        "{:013x}{:08x}",
        unix_millis & 0xF_FFFF_FFFF_FFFF,
        u32::from_le_bytes(r)
    )
}

// ---- lịch ----

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn day_number(t: &LocalTime) -> i64 {
    days_from_civil(t.year as i64, t.month as i64, t.day as i64)
}

pub fn expires_on(local: &LocalTime, expiry: Expiry) -> Option<String> {
    let add = match expiry {
        Expiry::D1 => 1,
        Expiry::D7 => 7,
        Expiry::D30 => 30,
        Expiry::Keep => return None,
    };
    let (y, m, d) = civil_from_days(day_number(local) + add);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn parse_date(s: &str) -> Option<(u32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let p = &s[r];
        if p.bytes().all(|c| c.is_ascii_digit()) {
            p.parse().ok()
        } else {
            None
        }
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if (1..=12).contains(&m) && (1..=31).contains(&d) {
        Some((y, m, d))
    } else {
        None
    }
}

pub fn is_expired(expires: &str, today: &LocalTime) -> bool {
    match parse_date(expires) {
        Some(e) => e < (today.year as u32, today.month as u32, today.day as u32),
        None => false,
    }
}

// ---- chỉ mục ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexError {
    Syntax,
    UnsupportedVersion(u32),
}

fn get_u64(v: &Value, k: &str) -> Option<u64> {
    v.get(k)?.as_u64()
}

fn opt_string(o: &serde_json::Map<String, Value>, k: &str) -> Result<Option<String>, ()> {
    match o.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(()),
    }
}

fn parse_entry(v: &Value) -> Option<Entry> {
    let o = v.as_object()?;
    let id = o.get("id")?.as_str()?;
    if !is_valid_id(id) {
        return None;
    }
    let kind = Kind::from_str(o.get("kind")?.as_str()?)?;
    let unix = get_u64(v, "unix")?;
    let l = o.get("local")?;
    let part = |k: &str, max: u64| -> Option<u64> { get_u64(l, k).filter(|&n| n <= max) };
    let local = LocalTime {
        year: part("year", u16::MAX as u64)? as u16,
        month: part("month", 255)? as u8,
        day: part("day", 255)? as u8,
        hour: part("hour", 255)? as u8,
        minute: part("minute", 255)? as u8,
        second: part("second", 255)? as u8,
    };
    let width = u32::try_from(get_u64(v, "width")?)
        .ok()
        .filter(|&n| n > 0)?;
    let height = u32::try_from(get_u64(v, "height")?)
        .ok()
        .filter(|&n| n > 0)?;
    let bytes = match o.get("bytes") {
        None | Some(Value::Null) => None,
        Some(b) => Some(b.as_u64()?),
    };
    let path = opt_string(o, "path").ok()?.map(PathBuf::from);
    let managed = o.get("managed").and_then(Value::as_bool).unwrap_or(false);
    let url = opt_string(o, "url").ok()?;
    let expires = opt_string(o, "expires").ok()?;
    Some(Entry {
        id: id.to_string(),
        kind,
        unix,
        local,
        width,
        height,
        bytes,
        path,
        managed,
        url,
        expires,
    })
}

/// Nạp khoan dung: trả (mục đọc được, số mục bị bỏ vì hỏng hoặc trùng id).
pub fn parse_index(text: &str) -> Result<(Vec<Entry>, usize), IndexError> {
    let root: Value = serde_json::from_str(text).map_err(|_| IndexError::Syntax)?;
    let obj = root.as_object().ok_or(IndexError::Syntax)?;
    match obj.get("version") {
        None => {}
        Some(v) => {
            let n = v.as_u64().ok_or(IndexError::Syntax)?;
            if n > INDEX_VERSION as u64 {
                return Err(IndexError::UnsupportedVersion(
                    u32::try_from(n).unwrap_or(u32::MAX),
                ));
            }
        }
    }
    let mut out: Vec<Entry> = Vec::new();
    let mut dropped = 0usize;
    if let Some(Value::Array(items)) = obj.get("entries") {
        for item in items {
            match parse_entry(item) {
                Some(e) if !out.iter().any(|x| x.id == e.id) => out.push(e),
                _ => dropped += 1,
            }
        }
    }
    out.truncate(MAX_ENTRIES);
    Ok((out, dropped))
}

pub fn serialize_index(entries: &[Entry]) -> String {
    let list: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "kind": e.kind.as_str(),
                "unix": e.unix,
                "local": {
                    "year": e.local.year,
                    "month": e.local.month,
                    "day": e.local.day,
                    "hour": e.local.hour,
                    "minute": e.local.minute,
                    "second": e.local.second,
                },
                "width": e.width,
                "height": e.height,
                "bytes": e.bytes,
                "path": e.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                "managed": e.managed,
                "url": e.url,
                "expires": e.expires,
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "version": INDEX_VERSION, "entries": list }))
        .unwrap_or_else(|_| String::from("{\"version\":1,\"entries\":[]}"))
}

// ---- tìm/lọc ----

fn fold_char(c: char, out: &mut String) {
    for l in c.to_lowercase() {
        out.push(match l {
            'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ' | 'ấ'
            | 'ẩ' | 'ẫ' | 'ậ' => 'a',
            'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' => {
                'e'
            }
            'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',
            'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ' | 'ớ'
            | 'ở' | 'ỡ' | 'ợ' => 'o',
            'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => {
                'u'
            }
            'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
            'đ' => 'd',
            other => other,
        });
    }
}

/// Hạ chữ thường và bỏ dấu tiếng Việt.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        fold_char(c, &mut out);
    }
    out
}

pub fn matches_query(e: &Entry, query: &str, lang: Lang) -> bool {
    let q = fold(query);
    let mut terms = q.split_whitespace().peekable();
    if terms.peek().is_none() {
        return true;
    }
    let hay = fold(&format!(
        "{}\n{}\n{:04}-{:02}-{:02}\n{:02}/{:02}/{:04}\n{}",
        e.file_name().unwrap_or_default(),
        e.kind.label(lang),
        e.local.year,
        e.local.month,
        e.local.day,
        e.local.day,
        e.local.month,
        e.local.year,
        e.url.as_deref().unwrap_or("")
    ));
    terms.all(|term| hay.contains(term))
}

pub fn filter_entries<'a>(
    entries: &'a [Entry],
    filter: Filter,
    query: &str,
    lang: Lang,
) -> Vec<&'a Entry> {
    entries
        .iter()
        .filter(|e| filter.accepts(e.kind) && matches_query(e, query, lang))
        .collect()
}

// ---- định dạng ----

pub fn format_size(lang: Lang, bytes: u64) -> String {
    const KB: f64 = 1024.0;
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = (bytes as f64 / KB).round() as u64;
    if kb < 1024 {
        return format!("{kb} KB");
    }
    let (value, unit) = {
        let mb = bytes as f64 / (KB * KB);
        if (mb * 10.0).round() / 10.0 < 1024.0 {
            (mb, "MB")
        } else {
            (bytes as f64 / (KB * KB * KB), "GB")
        }
    };
    let s = format!("{value:.1}");
    let s = match lang {
        Lang::Vi => s.replace('.', ","),
        Lang::En => s,
    };
    format!("{s} {unit}")
}

pub fn format_dims(w: u32, h: u32) -> String {
    format!("{w} \u{d7} {h}")
}

fn date_text(lang: Lang, w: &LocalTime) -> String {
    match lang {
        Lang::Vi => format!("{:02}/{:02}/{:04}", w.day, w.month, w.year),
        Lang::En => format!("{:04}-{:02}-{:02}", w.year, w.month, w.day),
    }
}

pub fn format_when(lang: Lang, when: &LocalTime, now: &LocalTime) -> String {
    let hm = format!("{:02}:{:02}", when.hour, when.minute);
    match day_number(now) - day_number(when) {
        0 => format!("{} {hm}", t(lang, Msg::HistoryToday)),
        1 => format!("{} {hm}", t(lang, Msg::HistoryYesterday)),
        _ => format!("{} {hm}", date_text(lang, when)),
    }
}

pub fn format_full(lang: Lang, when: &LocalTime) -> String {
    format!(
        "{} {:02}:{:02}:{:02}",
        date_text(lang, when),
        when.hour,
        when.minute,
        when.second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lt(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> LocalTime {
        LocalTime {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
            second: s,
        }
    }

    fn entry(id_n: u32, kind: Kind) -> Entry {
        Entry {
            id: format!("{:021x}", id_n),
            kind,
            unix: 1_700_000_000,
            local: lt(2026, 11, 18, 14, 32, 5),
            width: 1920,
            height: 1080,
            bytes: Some(1234),
            path: None,
            managed: false,
            url: None,
            expires: None,
        }
    }

    #[test]
    fn kind_and_filter() {
        for k in [
            Kind::Saved,
            Kind::Uploaded,
            Kind::Copied,
            Kind::UploadFallback,
        ] {
            assert_eq!(Kind::from_str(k.as_str()), Some(k));
            assert!(!k.label(Lang::Vi).is_empty());
            assert!(Filter::All.accepts(k));
        }
        assert_eq!(Kind::from_str("zzz"), None);
        assert!(Filter::Saved.accepts(Kind::UploadFallback));
        assert!(Filter::Saved.accepts(Kind::Saved));
        assert!(!Filter::Uploaded.accepts(Kind::Saved));
        assert!(Filter::Uploaded.accepts(Kind::Uploaded));
        assert!(Filter::Copied.accepts(Kind::Copied));
        assert!(!Filter::Copied.accepts(Kind::Uploaded));
    }

    #[test]
    fn ids() {
        let a = new_id(1_700_000_000_123);
        let b = new_id(1_700_000_000_123);
        assert!(is_valid_id(&a));
        assert_eq!(&a[..13], format!("{:013x}", 1_700_000_000_123u64));
        let c = crate::i18n::history_clear_confirm_body;
        assert!(c(Lang::Vi, 3).starts_with("3 mục"));
        assert!(c(Lang::En, 1).starts_with("1 entry will"));
        assert!(c(Lang::En, 2).starts_with("2 entries will"));
        assert_ne!(a, b);
        assert!(!is_valid_id("../x"));
        assert!(!is_valid_id("abc"));
        assert!(!is_valid_id(&"A".repeat(21)));
        assert!(!is_valid_id(&"a".repeat(22)));
        assert!(is_valid_id(&"a".repeat(21)));
    }

    #[test]
    fn entry_paths() {
        let mut e = entry(1, Kind::Saved);
        let root = Path::new("root");
        assert_eq!(e.image_path(root), None);
        let saved = Path::new("x").join("lưu.png");
        e.path = Some(saved.clone());
        assert_eq!(e.file_name().as_deref(), Some("lưu.png"));
        assert_eq!(e.image_path(root), Some(saved));
        e.managed = true;
        assert_eq!(
            e.image_path(root),
            Some(root.join("images").join(format!("{}.png", e.id)))
        );
        assert_eq!(
            e.thumb_path(root),
            root.join("thumbs").join(format!("{}.png", e.id))
        );
    }

    #[test]
    fn expiry_dates() {
        let d = |y, m, dd, x| expires_on(&lt(y, m, dd, 0, 0, 0), x);
        assert_eq!(d(2026, 11, 18, Expiry::D1).as_deref(), Some("2026-11-19"));
        assert_eq!(d(2026, 11, 18, Expiry::D7).as_deref(), Some("2026-11-25"));
        assert_eq!(d(2026, 11, 18, Expiry::D30).as_deref(), Some("2026-12-18"));
        assert_eq!(d(2026, 12, 20, Expiry::D30).as_deref(), Some("2027-01-19"));
        assert_eq!(d(2028, 2, 20, Expiry::D30).as_deref(), Some("2028-03-21"));
        assert_eq!(d(2026, 12, 31, Expiry::D1).as_deref(), Some("2027-01-01"));
        assert_eq!(d(2026, 11, 18, Expiry::Keep), None);
        let today = lt(2026, 11, 18, 0, 0, 0);
        assert!(is_expired("2026-11-17", &today));
        assert!(!is_expired("2026-11-18", &today));
        assert!(!is_expired("2026-11-19", &today));
        assert!(!is_expired("abc", &today));
        assert!(!is_expired("2026-13-01", &today));
    }

    fn full_entry() -> Entry {
        Entry {
            id: "00018bcfe56840123abcd".to_string(),
            kind: Kind::UploadFallback,
            unix: 1_700_000_000,
            local: lt(2026, 11, 18, 14, 32, 5),
            width: 800,
            height: 600,
            bytes: Some(4096),
            path: Some(PathBuf::from("C:\\a\\b.png")),
            managed: true,
            url: Some("https://x.example/a.png".into()),
            expires: Some("2026-12-18".into()),
        }
    }

    #[test]
    fn index_roundtrip() {
        let v = vec![full_entry(), entry(2, Kind::Copied)];
        let text = serialize_index(&v);
        let (back, dropped) = parse_index(&text).unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(back, v);
        assert!(text.contains("\"version\": 1"));
    }

    #[test]
    fn index_versions_and_syntax() {
        let (v, d) = parse_index("{\"entries\":[]}").unwrap();
        assert!(v.is_empty() && d == 0);
        assert_eq!(
            parse_index("{\"version\":2,\"entries\":[]}").unwrap_err(),
            IndexError::UnsupportedVersion(2)
        );
        assert_eq!(parse_index("{oops").unwrap_err(), IndexError::Syntax);
        assert_eq!(parse_index("[]").unwrap_err(), IndexError::Syntax);
    }

    #[test]
    fn index_lenient() {
        let good = serialize_index(&[full_entry()]);
        let mut val: Value = serde_json::from_str(&good).unwrap();
        let g = val["entries"][0].clone();
        let mut bad_id = g.clone();
        bad_id["id"] = json!("../x");
        let mut bad_kind = g.clone();
        bad_kind["kind"] = json!("weird");
        let mut zero = g.clone();
        zero["id"] = json!("00018bcfe56840123abce");
        zero["width"] = json!(0);
        let mut bad_path = g.clone();
        bad_path["id"] = json!("00018bcfe56840123abcf");
        bad_path["path"] = json!(5);
        let mut extra = g.clone();
        extra["id"] = json!("00018bcfe56840123abc0");
        extra["future"] = json!({"a": 1});
        let dup = g.clone();
        val["entries"] = json!([bad_id, g, bad_kind, zero, bad_path, dup, extra, 7]);
        let (v, d) = parse_index(&val.to_string()).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "00018bcfe56840123abcd");
        assert_eq!(v[1].id, "00018bcfe56840123abc0");
        assert_eq!(d, 6);
    }

    #[test]
    fn index_truncates() {
        let v: Vec<Entry> = (0..60).map(|i| entry(i, Kind::Saved)).collect();
        let (back, _) = parse_index(&serialize_index(&v)).unwrap();
        assert_eq!(back.len(), MAX_ENTRIES);
        assert_eq!(back[0].id, v[0].id);
    }

    #[test]
    fn folding() {
        assert_eq!(fold("Lưu Ảnh Đẹp"), "luu anh dep");
    }

    #[test]
    fn search() {
        let mut a = entry(1, Kind::Uploaded);
        a.path = Some(PathBuf::from("lưu_ảnh.png"));
        a.url = Some("https://cdn.example.com/abc123.png".into());
        let b = entry(2, Kind::Saved);
        assert!(matches_query(&a, "luu", Lang::Vi));
        assert!(matches_query(&a, "2026-11-18", Lang::Vi));
        assert!(matches_query(&a, "18/11/2026", Lang::Vi));
        assert!(matches_query(&a, "da tai len", Lang::Vi));
        assert!(matches_query(&a, "uploaded", Lang::En));
        assert!(!matches_query(&a, "uploaded", Lang::Vi));
        assert!(matches_query(&a, "luu example", Lang::Vi));
        assert!(!matches_query(&a, "luu zzz", Lang::Vi));
        assert!(matches_query(&a, "", Lang::Vi));
        assert!(matches_query(&a, "   ", Lang::Vi));
        assert!(matches_query(&a, "example.com/abc", Lang::Vi));
        let list = vec![b.clone(), a.clone(), entry(3, Kind::Uploaded)];
        let r = filter_entries(&list, Filter::Uploaded, "2026-11-18", Lang::En);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].id, a.id);
        assert_eq!(filter_entries(&list, Filter::All, "", Lang::En).len(), 3);
        assert_eq!(
            filter_entries(&list, Filter::Uploaded, "luu", Lang::En).len(),
            1
        );
    }

    #[test]
    fn sizes_and_dims() {
        assert_eq!(format_size(Lang::En, 0), "0 B");
        assert_eq!(format_size(Lang::En, 1023), "1023 B");
        assert_eq!(format_size(Lang::En, 1024), "1 KB");
        assert_eq!(format_size(Lang::En, 421_888), "412 KB");
        assert_eq!(format_size(Lang::En, 1_258_291), "1.2 MB");
        assert_eq!(format_size(Lang::Vi, 1_258_291), "1,2 MB");
        assert_eq!(format_size(Lang::En, 1_048_575), "1.0 MB");
        assert_eq!(format_dims(1920, 1080), "1920 × 1080");
    }

    #[test]
    fn when_formats() {
        let now = lt(2026, 11, 18, 20, 0, 0);
        assert_eq!(
            format_when(Lang::Vi, &lt(2026, 11, 18, 14, 32, 5), &now),
            "Hôm nay 14:32"
        );
        assert_eq!(
            format_when(Lang::En, &lt(2026, 11, 18, 14, 32, 5), &now),
            "Today 14:32"
        );
        assert_eq!(
            format_when(Lang::Vi, &lt(2026, 11, 17, 9, 10, 0), &now),
            "Hôm qua 09:10"
        );
        assert_eq!(
            format_when(
                Lang::En,
                &lt(2026, 10, 31, 9, 10, 0),
                &lt(2026, 11, 1, 1, 0, 0)
            ),
            "Yesterday 09:10"
        );
        assert_eq!(
            format_when(
                Lang::Vi,
                &lt(2025, 12, 31, 23, 0, 0),
                &lt(2026, 1, 1, 0, 5, 0)
            ),
            "Hôm qua 23:00"
        );
        assert_eq!(
            format_when(Lang::Vi, &lt(2026, 11, 16, 14, 32, 0), &now),
            "16/11/2026 14:32"
        );
        assert_eq!(
            format_when(
                Lang::En,
                &lt(2026, 11, 18, 14, 32, 0),
                &lt(2026, 11, 20, 0, 0, 0)
            ),
            "2026-11-18 14:32"
        );
        assert_eq!(
            format_full(Lang::Vi, &lt(2026, 11, 18, 14, 32, 5)),
            "18/11/2026 14:32:05"
        );
        assert_eq!(
            format_full(Lang::En, &lt(2026, 11, 18, 14, 32, 5)),
            "2026-11-18 14:32:05"
        );
    }
}
