//! Kiểm tra bản mới qua GitHub releases. Mọi lỗi đều im lặng (trả `None`).

use std::io::Read;
use std::time::Duration;

pub const LATEST_URL: &str =
    "https://api.github.com/repos/tuanluu-agilityio/darkshot/releases/latest";
pub const RELEASE_URL_PREFIX: &str = "https://github.com/tuanluu-agilityio/darkshot/";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(20);
const BODY_LIMIT: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// "1.2.3" hoặc "v1.2.3" (đúng ba số, không thừa ký tự); khác thì None.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let mut parts = s.split('.');
        let mut num = || {
            let p = parts.next()?;
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            p.parse::<u32>().ok()
        };
        let (major, minor, patch) = (num()?, num()?, num()?);
        if parts.next().is_some() {
            return None;
        }
        Some(Version {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub url: String,
}

/// Đọc JSON của /releases/latest. None khi: JSON lỗi, thiếu tag_name hoặc html_url,
/// draft hoặc prerelease là true, tag không đọc được, html_url không bắt đầu bằng RELEASE_URL_PREFIX.
pub fn parse_latest(json: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let obj = v.as_object()?;
    for flag in ["draft", "prerelease"] {
        match obj.get(flag) {
            None | Some(serde_json::Value::Bool(false)) => {}
            _ => return None,
        }
    }
    let version = Version::parse(obj.get("tag_name")?.as_str()?)?;
    let url = obj.get("html_url")?.as_str()?;
    let safe = url.starts_with(RELEASE_URL_PREFIX)
        && !url.contains("..")
        && url.bytes().all(|b| b.is_ascii_graphic());
    if !safe {
        return None;
    }
    Some(Release {
        version,
        url: url.to_string(),
    })
}

/// Release mới hơn `current`, hoặc None.
pub fn newer_than(current: Version, latest: Option<Release>) -> Option<Release> {
    latest.filter(|r| r.version > current)
}

/// GET `url` với User-Agent "darkshot/<current>" và Accept "application/vnd.github+json";
/// chờ kết nối 10 s, tổng 20 s. Mã 200 thì trả parse_latest(body); mọi lỗi khác trả None.
/// Đọc thân tối đa 256 KiB.
pub fn fetch_latest(url: &str, current: Version) -> Option<Release> {
    // Roots: webpki-roots (TlsConfig mặc định). Không proxy môi trường, không theo chuyển hướng.
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .proxy(None)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut resp = agent
        .get(url)
        .header("User-Agent", format!("darkshot/{current}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    let mut buf = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(BODY_LIMIT + 1)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.len() as u64 > BODY_LIMIT {
        return None;
    }
    parse_latest(std::str::from_utf8(&buf).ok()?)
}

/// Luật hiển thị khi kiểm tra bản mới xong: (hiện mục menu, hiện thông báo).
/// Có bản mới thì luôn hiện mục menu; chỉ thông báo khi chưa báo đúng bản đó.
pub fn update_outcome(found: Option<&Release>, notified: Option<Version>) -> (bool, bool) {
    match found {
        None => (false, false),
        Some(r) => (true, notified != Some(r.version)),
    }
}

/// Mục "Có bản mới" sau một lần kiểm tra: lỗi hoặc không có bản mới (`None`) không xóa mục đã
/// hiện (lỗi mạng ở lần kiểm tra sau không được làm mất nó); chỉ có bản mới thì ghi đè.
pub fn update_after_check(current: Option<Release>, found: Option<Release>) -> Option<Release> {
    found.or(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upload::PORT_LOCK;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn v(major: u32, minor: u32, patch: u32) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn version_parse() {
        assert_eq!(Version::parse("0.2.0"), Some(v(0, 2, 0)));
        assert_eq!(Version::parse("v0.2.0"), Some(v(0, 2, 0)));
        assert_eq!(Version::parse("10.20.30"), Some(v(10, 20, 30)));
        assert_eq!(Version::parse("01.2.3"), Some(v(1, 2, 3)));
        for bad in [
            "",
            "v",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "v1.2.3-beta",
            " 1.2.3",
            "01.2.3x",
            "vv1.2.3",
            "1..3",
            "+1.2.3",
            "-1.2.3",
            "99999999999.0.0",
            "1.2.3 ",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
        assert!(v(0, 10, 0) > v(0, 9, 9));
        assert!(v(1, 0, 0) > v(0, 99, 99));
        assert_eq!(v(0, 2, 0).to_string(), "0.2.0");
    }

    fn sample_json(tag: &str, url: &str, draft: bool, pre: bool) -> String {
        format!(
            r#"{{"url":"https://api.github.com/repos/x/y/releases/1","tag_name":"{tag}","name":"darkshot {tag}","draft":{draft},"prerelease":{pre},"html_url":"{url}","assets":[{{"name":"darkshot.exe","size":123}}],"body":"notes"}}"#
        )
    }

    #[test]
    fn parse_latest_cases() {
        let good_url = "https://github.com/tuanluu-agilityio/darkshot/releases/tag/v0.3.0";
        assert_eq!(
            parse_latest(&sample_json("v0.3.0", good_url, false, false)),
            Some(Release {
                version: v(0, 3, 0),
                url: good_url.to_string()
            })
        );
        assert_eq!(
            parse_latest(&sample_json("v0.3.0", good_url, true, false)),
            None
        );
        assert_eq!(
            parse_latest(&sample_json("v0.3.0", good_url, false, true)),
            None
        );
        for bad_url in [
            "http://github.com/tuanluu-agilityio/darkshot/releases/tag/v0.3.0",
            "https://github.com/other/darkshot/releases/tag/v0.3.0",
            "https://github.com/tuanluu-agilityio/darkshot2/releases/tag/v0.3.0",
            "https://github.com/tuanluu-agilityio/darkshot/../../evil/x",
            "https://github.com/tuanluu-agilityio/darkshot/a b",
            "https://evil.com/https://github.com/tuanluu-agilityio/darkshot/",
            "",
        ] {
            assert_eq!(
                parse_latest(&sample_json("v0.3.0", bad_url, false, false)),
                None,
                "{bad_url:?}"
            );
        }
        // Tag không đọc được.
        for tag in ["v1.2", "latest", "v1.2.3-beta", ""] {
            assert_eq!(
                parse_latest(&sample_json(tag, good_url, false, false)),
                None,
                "{tag:?}"
            );
        }
        // Thiếu trường, sai kiểu, JSON hỏng.
        for bad in [
            "",
            "{",
            "[]",
            "null",
            r#"{"tag_name":"v0.3.0"}"#,
            &format!(r#"{{"html_url":"{good_url}"}}"#),
            &format!(r#"{{"tag_name":3,"html_url":"{good_url}"}}"#),
            r#"{"tag_name":"v0.3.0","html_url":5}"#,
            &format!(r#"{{"tag_name":"v0.3.0","html_url":"{good_url}","draft":"no"}}"#),
            &format!(r#"{{"tag_name":"v0.3.0","html_url":"{good_url}","prerelease":1}}"#),
            &sample_json("v0.3.0", good_url, false, false)[..40],
        ] {
            assert_eq!(parse_latest(bad), None, "{bad:?}");
        }
        // Thiếu draft/prerelease: coi như bản thường.
        assert!(parse_latest(&format!(
            r#"{{"tag_name":"v0.3.0","html_url":"{good_url}"}}"#
        ))
        .is_some());
        // Rất lớn, lồng sâu: không panic.
        let huge = format!("{}1{}", "[".repeat(100_000), "]".repeat(100_000));
        assert_eq!(parse_latest(&huge), None);
        let long = format!(
            r#"{{"tag_name":"{}","html_url":"x"}}"#,
            "9".repeat(1_000_000)
        );
        assert_eq!(parse_latest(&long), None);
    }

    #[test]
    fn newer_than_cases() {
        let rel = |a, b, c| {
            Some(Release {
                version: v(a, b, c),
                url: format!("{RELEASE_URL_PREFIX}releases/tag/v{a}.{b}.{c}"),
            })
        };
        assert_eq!(newer_than(v(0, 2, 0), rel(0, 2, 0)), None);
        assert_eq!(newer_than(v(0, 2, 0), rel(0, 1, 9)), None);
        assert_eq!(newer_than(v(0, 3, 0), rel(0, 2, 9)), None);
        assert_eq!(newer_than(v(0, 2, 0), None), None);
        assert_eq!(newer_than(v(0, 2, 0), rel(0, 2, 1)), rel(0, 2, 1));
        assert_eq!(newer_than(v(0, 2, 0), rel(1, 0, 0)), rel(1, 0, 0));
    }

    struct Mock {
        addr: String,
        headers: Arc<Mutex<Vec<(String, String)>>>,
    }

    /// Máy chủ một lượt: ghi header của yêu cầu rồi trả `code` và `body`.
    fn spawn_mock(code: u16, body: String) -> Mock {
        let listener = {
            let _g = PORT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            TcpListener::bind("127.0.0.1:0").unwrap()
        };
        let addr = listener.local_addr().unwrap().to_string();
        let headers = Arc::new(Mutex::new(Vec::new()));
        let h2 = headers.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let mut g = h2.lock().unwrap();
                for l in head.split("\r\n").skip(1) {
                    if let Some((k, v)) = l.split_once(':') {
                        g.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
                    }
                }
                drop(g);
                let head = format!(
                    "HTTP/1.1 {code} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(body.as_bytes());
            }
        });
        Mock { addr, headers }
    }

    #[test]
    fn fetch_latest_local() {
        let url = "https://github.com/tuanluu-agilityio/darkshot/releases/tag/v0.3.0";
        let m = spawn_mock(200, sample_json("v0.3.0", url, false, false));
        let got = fetch_latest(&format!("http://{}/x", m.addr), v(0, 2, 0));
        assert_eq!(
            got,
            Some(Release {
                version: v(0, 3, 0),
                url: url.to_string()
            })
        );
        let hs = m.headers.lock().unwrap().clone();
        let get = |n: &str| hs.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
        assert_eq!(get("user-agent").as_deref(), Some("darkshot/0.2.0"));
        assert_eq!(
            get("accept").as_deref(),
            Some("application/vnd.github+json")
        );
        assert_eq!(get("authorization"), None);

        let m = spawn_mock(404, "{}".into());
        assert_eq!(
            fetch_latest(&format!("http://{}/", m.addr), v(0, 2, 0)),
            None
        );

        let m = spawn_mock(200, "a".repeat(1024 * 1024));
        assert_eq!(
            fetch_latest(&format!("http://{}/", m.addr), v(0, 2, 0)),
            None
        );

        // Cổng đóng.
        let _g = PORT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        assert_eq!(
            fetch_latest(&format!("http://127.0.0.1:{port}/"), v(0, 2, 0)),
            None
        );
    }

    #[test]
    fn update_after_check_keeps_item_on_none() {
        let rel = |p| Release {
            version: Version {
                major: 0,
                minor: 3,
                patch: p,
            },
            url: String::new(),
        };
        assert_eq!(update_after_check(None, None), None);
        assert_eq!(update_after_check(Some(rel(0)), None), Some(rel(0)));
        assert_eq!(update_after_check(None, Some(rel(1))), Some(rel(1)));
        assert_eq!(update_after_check(Some(rel(0)), Some(rel(1))), Some(rel(1)));
    }

    #[test]
    fn update_outcome_rules() {
        let v = |m, n, p| Version {
            major: m,
            minor: n,
            patch: p,
        };
        let rel = Release {
            version: v(0, 3, 0),
            url: String::new(),
        };
        assert_eq!(update_outcome(None, None), (false, false));
        assert_eq!(update_outcome(None, Some(v(0, 3, 0))), (false, false));
        assert_eq!(update_outcome(Some(&rel), None), (true, true));
        assert_eq!(update_outcome(Some(&rel), Some(v(0, 3, 0))), (true, false));
        assert_eq!(update_outcome(Some(&rel), Some(v(0, 2, 0))), (true, true));
    }
}
