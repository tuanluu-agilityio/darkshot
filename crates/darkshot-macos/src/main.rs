//! Điểm vào của darkshot cho macOS (Apple silicon, macOS 14+). Vỏ ứng dụng ở `app.rs` (Task 104).

#[cfg(target_os = "macos")]
mod annotate_draw;
#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod capture;
#[cfg(target_os = "macos")]
mod clipboard;
// `coords` có hàm đổi vùng điểm sang pixel và pixel sang điểm chỉ dùng trong kiểm thử.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod coords;
#[cfg(target_os = "macos")]
mod hotkey;
#[cfg(target_os = "macos")]
mod hotkey_field;
#[cfg(target_os = "macos")]
mod keymap;
#[cfg(target_os = "macos")]
mod lang;
#[cfg(target_os = "macos")]
mod notify;
#[cfg(target_os = "macos")]
mod options;
#[cfg(target_os = "macos")]
mod options_model;
#[cfg(target_os = "macos")]
mod options_view;
#[cfg(target_os = "macos")]
mod overlay;
#[cfg(target_os = "macos")]
mod overlay_input;
#[cfg(target_os = "macos")]
mod overlay_view;
// `paths` có các đường dẫn cho tính năng sau (lịch sử, chụp từng bước, lần chụp trước).
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod paths;
#[cfg(target_os = "macos")]
mod secret;
#[cfg(target_os = "macos")]
mod shutdown;
#[cfg(target_os = "macos")]
mod timeutil;
#[cfg(target_os = "macos")]
mod tray;
#[cfg(target_os = "macos")]
mod upload;

#[cfg(target_os = "macos")]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(app::run(&args));
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("darkshot-macos runs on macOS only.");
    std::process::exit(1);
}
