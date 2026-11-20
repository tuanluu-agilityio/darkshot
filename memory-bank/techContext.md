# Bối cảnh kỹ thuật

Cập nhật: 2026-11-16

## Môi trường phát triển

- Windows 11, 64-bit. Thư mục dự án: `D:\2026 - Tuan\shotme` (đường dẫn có dấu cách, luôn đặt trong dấu nháy).
- Rust 1.100.0 stable, target `x86_64-pc-windows-msvc`, cài qua rustup. Có sẵn `clippy` và `rustfmt`.
- Visual Studio 2022 Build Tools, workload C++ (cung cấp linker MSVC).
- Git 2.47. Nhánh chính `main`. Remote `origin`: repo GitHub công khai `tuanluu-agilityio/darkshot` (đã đổi tên, xem quyết định 53 về địa chỉ cục bộ).
- `cargo` nằm ở `%USERPROFILE%\.cargo\bin`. Nếu shell chưa thấy `cargo`, thêm thư mục đó vào `PATH` của phiên.

## Lệnh

| Việc | Lệnh |
|---|---|
| Cổng kiểm tra trước khi gộp | `powershell -File scripts/check.ps1` (phải in `ALL CHECKS PASSED`) |
| Định dạng mã trước khi commit | `cargo fmt --all` |
| Kiểm thử một crate | `cargo test -p darkshot-core <bộ lọc>` |
| Kiểm thử chạy tay ba cửa sổ (mở cửa sổ thật, cần người nhìn) | `cargo test -p darkshot-windows -- --ignored about_window help_window settings_window --test-threads=1` |
| Kiểm thử chạy tay chụp con trỏ (chụp màn hình thật, kiểm GDI) | `cargo test -p darkshot-windows -- --ignored capture_with_cursor --test-threads=1` |
| Build bản phát hành | `cargo build --release` (ra `target/release/darkshot.exe`) |
| Thêm crate | `cargo add -p <crate của dự án> <tên>` |

## Thư viện dự kiến

| Crate | Dùng ở | Để làm gì |
|---|---|---|
| `serde`, `serde_json`, `base64` | core | file cấu hình |
| `getrandom` | core | tên ngẫu nhiên |
| `sha2`, `hmac` | core | chữ ký SigV4 |
| `ureq` (chỉ `rustls` + `webpki-roots`) | core | HTTPS tới R2 |
| `png` | core | mã hóa ảnh |
| `jpeg-encoder` | core | nén JPEG cho ảnh tải lên (ghim `=0.7.1`, không bật `simd`) |
| `windows` | windows | Win32 |
| `embed-resource` hoặc tương đương | windows | nhúng biểu tượng và manifest |

Dùng bản mới nhất tại thời điểm thêm. Phiên bản thực tế nằm trong `Cargo.lock`.

## Ràng buộc kỹ thuật

- `darkshot-core` không phụ thuộc crate `windows`, `native-tls`, `schannel`, `rustls-platform-verifier`.
- Không dùng TLS của hệ điều hành.
- Profile release: `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
- R2: địa chỉ S3 `https://{account_id}.r2.cloudflarestorage.com`, vùng `auto`, dịch vụ `s3`, đường dẫn `/{bucket}/{key}`.
- Phiên đám mây của Claude Code chạy Ubuntu nên không build được phần Windows; dự án làm trên máy local.

## Phát sinh

- Task 3: `serde`, `serde_json`, `base64`, `getrandom`. Task 4: `sha2` 0.11, `hmac` 0.13.
- Task 5: `ureq` 3.5.2 với `default-features = false, features = ["rustls"]` (provider `ring`, `webpki-roots`). Agent HTTP đặt `proxy(None)`, `max_redirects(0)`, `http_status_as_error(false)`, thời hạn kết nối 10 giây, toàn bộ 60 giây.
- Task 6: `png` 0.18. Task 7: `windows` 0.62.2 (features: Win32_Foundation, Win32_UI_WindowsAndMessaging, Win32_UI_Shell, Win32_UI_Input_KeyboardAndMouse, Win32_System_LibraryLoader, Win32_System_Threading, Win32_System_SystemInformation, Win32_System_Com, Win32_Graphics_Gdi, Win32_Security), `embed-resource` 3.0.12 (build).
- Cổng kiểm tra có thêm `cargo fmt --all --check` (từ sau Task 5).
- Đợt 1 (Task 16): `scripts/check.ps1` có bước `no hard-coded Vietnamese strings`. Bước này quét `crates/*/src/*.rs`, bỏ qua `i18n.rs`, tìm chuỗi có chữ Latin có dấu (U+00C0–U+024F, U+1E00–U+1EFF) viết thẳng trong mã, và dừng ở `#[cfg(test)]` đứng trước `mod`. Chuỗi English viết thẳng không bị bắt. Crate `windows` bật thêm `Win32_Globalization` (Task 16, `lang::os_default`) và `Win32_UI_Controls` (Task 17, dải tab `SysTabControl32`).
- Đợt 2 (Task 20–22): không thêm crate hay feature Win32 nào; `crates/darkshot-windows/Cargo.toml` giữ nguyên so với trước đợt 2. Con trỏ chuột lấy bằng `GetCursorInfo`, `GetIconInfo`, `DrawIconEx` (đã có trong feature `Win32_UI_WindowsAndMessaging` và `Win32_Graphics_Gdi`).
- Đợt 6 (Task 44–48): không thêm crate và không thêm feature Win32 nào; `Cargo.toml` không đổi. `darkshot_core::edit` và `geometry::side_layout` nằm trong core, không phụ thuộc crate `windows`.
- Lệnh kiểm thử chạy tay: `cargo test -p darkshot-windows -- --ignored about_window help_window settings_window --test-threads=1` chạy `about_window_opens_and_closes`, `help_window_opens_and_closes`, `settings_window_opens_and_cancels`. Các kiểm thử này mở cửa sổ thật trên máy, nên không nằm trong `scripts/check.ps1`.
- Đợt 3 (Task 24): thêm crate `jpeg-encoder` `=0.7.1` vào `darkshot-core`, không bật `simd`, không phụ thuộc crate nào khác, giấy phép `(MIT OR Apache-2.0) AND IJG`. Đây là ngoại lệ duy nhất của luật không thêm crate (`memory-bank/decisions.md` mục 40). Không thêm feature Win32 nào trong đợt 3. Kiểm thử `capture::tests::jpeg_decodes_with_gdiplus` giải nén JPEG bằng GDI+ (feature `Win32_Graphics_GdiPlus` có từ trước) và chạy thường, không `#[ignore]`, nên không có lệnh `--ignored` mới.
- Đợt 5 (Task 34–37): không thêm crate. Crate `windows` thêm feature `Win32_UI_Input_Ime` (Task 37: `ImmGetContext`, `ImmSetCompositionWindow`, `ImmSetCompositionFontW`, `ImmGetCompositionStringW`, `ImmNotifyIME` để đặt cửa sổ soạn cạnh con trỏ ô chữ và chốt chuỗi đang soạn). Vẽ đánh dấu dùng GDI+ (`Win32_Graphics_GdiPlus`, có từ trước); dán chữ dùng `Win32_System_DataExchange` có sẵn (`clipboard::paste_text`). Kiểm thử chạy tay lớp phủ đánh dấu: `cargo test -p darkshot-windows -- --ignored overlay_round_trip_without_gdi_leaks --test-threads=1`, chạy từng cái một.
- Đợt 7 (Task 52–57): không thêm crate và không thêm feature Win32 nào; `Cargo.toml` không đổi (`FOLDERID_LocalAppData` đã có trong `Win32_UI_Shell`; giải mã PNG dùng crate `png` sẵn có). `getrandom` dùng cho mã mục lịch sử. Dữ liệu lịch sử ở `%LOCALAPPDATA%\darkshot\history\`. Biến môi trường `DARKSHOT_PROFILE_DIR` và cờ `--history` chỉ để chạy thử. Kiểm thử chạy tay mới: `cargo test -p darkshot-windows -- --ignored history_window --test-threads=1` (từng cái), và `make_demo_store` dựng dữ liệu demo (`DARKSHOT_DEMO_DIR`).
- Đợt 8: không thêm crate, không thêm feature `windows` (dùng `Win32_Graphics_Dwm`, `Win32_UI_Controls` có sẵn). Exe release 2.016.256 byte (đợt 7: 1.999.360, +16,9 KB). `enumerate_windows` đo 0,11–0,74 ms với 10–11 cửa sổ.

## Máy macOS (nhánh `feat/macos-port`)

- Apple silicon (`aarch64-apple-darwin`), macOS 15, Command Line Tools. Rust 1.100.0 cài qua rustup (profile minimal + clippy + rustfmt).
- M0 (2026-11-21): `cargo test -p darkshot-core` đạt 328/328, `fmt` và `clippy -D warnings` sạch. Một test (`history::tests::entry_paths`) từng giả định dấu `\` là dấu phân tách đường dẫn; đã sửa bằng `Path::join`.
- `cargo build --workspace` HỎNG trên macOS vì crate `windows` không biên dịch; trên Mac dùng `-p <crate>` cho đến khi M2 xong.
- Nạp cargo vào shell: `. "$HOME/.cargo/env"`.
