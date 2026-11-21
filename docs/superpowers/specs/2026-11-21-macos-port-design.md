# Đợt M: bản macOS (Apple silicon) — thiết kế

Ngày: 2026-11-21 · Nhánh tích hợp: `feat/macos-port` · Trạng thái: bản nháp chờ chủ dự án duyệt

Căn cứ: spec gốc mục 10 (kiến trúc hai lớp), quyết định 12. Thiết kế này không sửa spec đã duyệt; các thay đổi ghi ở `memory-bank/decisions.md` (quyết định 127 trở đi).

## 1. Mục tiêu và phạm vi

Thêm crate `darkshot-macos` chạy trên macOS 14 (Sonoma) trở lên, Apple silicon (`aarch64-apple-darwin`), dùng chung `darkshot-core`. Giao diện luôn tối, hai ngôn ngữ, mọi chuỗi qua `darkshot_core::i18n`.

**Giai đoạn M-A (bản dùng được, M3–M4):** biểu tượng trên thanh menu, phím tắt toàn cục, chụp vùng và toàn màn hình, lớp phủ chọn vùng, lưu PNG, chép vào clipboard, tải lên R2 và chép link có hạn, thông báo, cửa sổ Tùy chọn (Chung, Phím tắt, R2), khóa R2 trong Keychain, tải lại lần chụp trước.

**Giai đoạn M-B (từng tính năng độc lập, M5–M7):** đánh dấu trên ảnh, Ghim, Lịch sử, bắt cửa sổ, OCR, Chụp từng bước, tự khởi động, kiểm tra bản mới.

**Ngoài phạm vi:** Intel (x86_64), Linux, App Store, phát hành công khai có notarize (chỉ khi chủ dự án yêu cầu sau này), Chụp cuộn.

## 2. Phát hiện từ khảo sát mã (2026-11-21)

- `darkshot-core` sạch API Windows. Trên Apple silicon: 328/328 test đạt, `fmt` và `clippy -D warnings` sạch. Một test đã sửa vì giả định `\` là dấu phân tách.
- `cargo build --workspace` hỏng trên macOS vì crate `windows` không biên dịch.
- Lớp Windows (~44.700 dòng) chứa **logic thuần có test nằm trong file giao diện**: `main.rs` (`gate_step`, `repeat_plan`, `remember_after`, `startup_mode`, `parse_args`, `update_outcome`, `merge_reason`), `overlay.rs` (bố cục thanh công cụ, màu, kích cỡ), `capture.rs` (`to_image_coords`, `cursor_draw_origin`), `paths.rs` (`resolve`, tên file), `history_layout.rs`, `steps_*`. Bản Mac cần cùng hành vi; sao chép sẽ lệch dần.
- `Image` là BGRA từ trên xuống. ScreenCaptureKit và Core Graphics cho BGRA nên không cần đổi định dạng.

## 3. Quyết định kiến trúc

1. **Chuyển logic thuần sang core trước (M2).** Mỗi hàm thuần hiện ở lớp Windows mà bản Mac cũng cần được chuyển sang module core tương ứng, giữ nguyên test, lớp Windows gọi lại. Không đổi hành vi Windows; kiểm chứng bằng `check.ps1` trên Windows trước khi gộp. Nếu một hàm trộn logic với Win32 thì tách phần thuần ra, không chuyển nguyên khối.
2. **Không trừu tượng hóa giao diện.** Core không có cửa sổ trừu tượng. Mỗi nền tảng tự vẽ và gọi core qua các trait đã có (`SecretStore`, `Clipboard`, `Notifier`). Đường dẫn dữ liệu: core có `dirs.rs` (hàm thuần, Task 101) và mỗi nền tảng có `paths.rs` riêng như lớp Windows; không có trait `Dirs`. Lý do: quyết định 12 và quy ước "nền tảng chỉ vẽ"; một lớp UI chung sẽ nặng và không còn "Rust thuần, nhẹ".
3. **Workspace chạy được trên cả hai hệ điều hành.** `darkshot-windows` đưa `windows` vào `[target.'cfg(windows)'.dependencies]` và bọc mã bằng `#[cfg(windows)]`, với `main` rỗng cho hệ khác; `darkshot-macos` làm tương tự với `cfg(target_os = "macos")`. Khi đó `cargo test --workspace` chạy được ở cả hai nơi mà không cần `default-members`.
4. **AppKit thuần qua `objc2`** (`objc2`, `objc2-foundation`, `objc2-app-kit`, `objc2-core-graphics`, `objc2-screen-capture-kit`, `objc2-vision`, `objc2-security`); không thêm thư viện cửa sổ đa nền tảng. Mọi API Objective-C bọc trong module nhỏ có `// SAFETY:` rõ ràng.
5. **Tọa độ.** Core dùng pixel. Mac dùng "điểm" (point) và hệ trục Y lên. Lớp Mac đổi tại một chỗ (`coords.rs`) và có test: điểm ↔ pixel theo `backingScaleFactor`, lật trục Y giữa Cocoa và `Image` (từ trên xuống).

## 4. Ánh xạ nền tảng

| Việc | Windows | macOS |
|---|---|---|
| Khay | `Shell_NotifyIconW` | `NSStatusItem` + `NSMenu` |
| Phím tắt toàn cục | `RegisterHotKey` | Carbon `RegisterEventHotKey` (không cần quyền Accessibility); phím Option/Command trong `Hotkey` đổi qua một bảng ánh xạ |
| Chụp màn hình | GDI, một ảnh cho cả màn hình ảo | ScreenCaptureKit (`SCScreenshotManager.captureImage`, macOS 14+), MỖI MÀN HÌNH MỘT ẢNH ở pixel vật lý, cần quyền Screen Recording (quyết định 134, 135) |
| Lớp phủ chọn vùng | cửa sổ layered | `NSWindow` không viền, mức `screenSaver`, mỗi `NSScreen` một cửa sổ; vẽ bằng Core Graphics |
| Clipboard | `CF_DIB`, `CF_UNICODETEXT` | `NSPasteboard` (PNG + chuỗi) |
| Thông báo | bong bóng khay | `UNUserNotificationCenter` |
| Khóa R2 | DPAPI | Keychain qua crate `security-framework` (service `darkshot`, tài khoản cố định); `protect` trả dấu `keychain:v1` (quyết định 136) |
| Dữ liệu | `%APPDATA%\darkshot`, `%LOCALAPPDATA%\darkshot` | `~/Library/Application Support/darkshot`; ảnh `~/Pictures/darkshot`; `DARKSHOT_PROFILE_DIR` vẫn ghi đè |
| Tự khởi động | Registry Run | `SMAppService.mainApp` |
| Ghim ảnh | cửa sổ topmost | `NSPanel` nổi, không lấy tiêu điểm |
| OCR | `Windows.Media.Ocr` | `VNRecognizeTextRequest`; không gọi trên luồng giao diện (quyết định 119) |
| Bắt cửa sổ | `EnumWindows` | `CGWindowListCopyWindowInfo` |
| Giao diện tối | tự vẽ | `NSAppearance(.darkAqua)` ép cho mọi cửa sổ |

Ứng dụng là agent (`LSUIElement = true`), không có biểu tượng Dock.

## 5. Quyền riêng tư và bảo mật

- Không thêm điểm gửi dữ liệu mới: vẫn chỉ hai địa chỉ R2 đã cấu hình (và kiểm tra bản mới nếu bật, như Windows).
- Khóa R2 chỉ ở Keychain; không vào log, tệp cấu hình hay lời gọi dịch vụ ngoài.
- Quyền Screen Recording do hệ điều hành cấp; thiếu quyền thì hiện hướng dẫn mở System Settings, không chụp ra ảnh rỗng im lặng.
- Ký bằng chứng chỉ tự ký cục bộ ổn định (biến môi trường `DARKSHOT_SIGN_IDENTITY`) cho bản dùng nội bộ; ad-hoc (`-`) chỉ là phương án cuối vì danh tính đổi mỗi lần build làm mất quyền Screen Recording và Keychain (quyết định 137). Không đưa chứng chỉ, profile hay Team ID vào repo công khai.

## 6. Kiểm thử và cổng

- Logic mới và logic chuyển sang core có test trước khi viết mã (TDD, như quy ước).
- `scripts/check.sh` (mới) làm đúng việc `check.ps1`: `fmt --check`, `test --workspace`, `clippy -D warnings`, kiểm core không phụ thuộc crate `windows|native-tls|schannel|rustls-platform-verifier`, và kiểm không có chuỗi tiếng Việt viết cứng. Hai script phải giữ cùng danh sách kiểm; ghi chú chéo ở đầu mỗi file.
- Phần Mac không giả lập chuột hoặc phím thật khi chủ dự án dùng máy (quyết định 126). Demo cách ly: `scripts/demo-macos.sh` với `DARKSHOT_PROFILE_DIR` riêng.
- Chủ dự án không có máy Windows (2026-11-21). Biên dịch chéo từ Mac không dùng được (`ring` cần header C của MSVC). Cổng cho thay đổi chạm lớp Windows là job `windows` của `.github/workflows/check.yml`, chạy `check.ps1` trên runner `windows-latest`; M2 chỉ gộp vào `main` khi job đó xanh.

## 7. Rủi ro

| Rủi ro | Cách giảm |
|---|---|
| Lớp phủ và tọa độ Retina/đa màn hình sai | `coords.rs` thuần, có test; demo cách ly mỗi màn hình |
| ScreenCaptureKit cần quyền, chụp ngoài ý muốn | Kiểm quyền trước khi chụp, báo rõ khi thiếu |
| Chuyển logic sang core làm lệch Windows | Giữ nguyên test, chạy `check.ps1` trước khi gộp, tách task nhỏ |
| `objc2` đổi API nhanh | Ghim phiên bản chính xác trong `Cargo.toml` |
| Không có máy Windows để xác nhận M2 | CI `windows-latest` chạy `check.ps1`; không gộp `main` khi chưa xanh |

## 8. Việc chủ dự án cần duyệt

Phạm vi M-A trước, M-B sau; AppKit thuần; ký cục bộ, không notarize; macOS 14+; nhánh `feat/macos-port` là nhánh tích hợp và task tách ra thành `task/1NN-…`; đánh số task từ 100.

## 9. Sửa đổi sau khảo sát M3 (2026-11-21)

- Yêu cầu tối thiểu nâng từ macOS 13 lên **14**: `SCScreenshotManager` (chụp một khung hình) chỉ có từ 14; chạy 13 phải dùng `SCStream` phức tạp hơn nhiều (quyết định 134).
- Chụp **từng màn hình**, vùng chọn không vắt qua màn hình (quyết định 135): các màn hình Mac có hệ số Retina khác nhau nên không có một canvas pixel chung rõ nghĩa như `Screenshot` của lớp Windows. Hệ quả: `windows`/`repeat` theo toạ độ ảo (M-B) sẽ cần thiết kế lại cho Mac.
- Gói `.app` và ký ổn định chuyển lên M3 (quyết định 137): thông báo và quyền hệ thống gắn với bundle và danh tính ký.
