# Task 104: vỏ ứng dụng macOS: `.app`, thanh menu, chụp từng màn hình (đợt M, bước M3b)

**Agent:** `builder` · **Model:** opus · **Nhánh:** `task/104-macos-app-shell` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (fable) · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml` (Windows chỉ biên dịch phần khung của crate Mac) + QA tay của chủ dự án (dưới)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` (mục 3, 4, 5 và 9). Quyết định 127–137 (đặc biệt 134–137). Tiền lệ cách làm: `docs/tasks/101-core-shared-logic.md`.

## Mục tiêu

Một ứng dụng chạy được trên máy thật: biểu tượng trên thanh menu, menu "Chụp" chụp màn hình bằng ScreenCaptureKit, xin quyền đúng cách, lưu PNG, thông báo kết quả. **Chưa có lớp phủ chọn vùng** (Task 105): ở Task này, "Chụp" lưu cả màn hình chứa con trỏ ra thư mục lưu để kiểm chứng toàn bộ đường ống (đường tạm này bị thay bằng lớp phủ ở Task 105).

## Cần đọc

`crates/darkshot-core/src/{tray_logic,capture_gate,app_state,app_flow,notices,actions,config,i18n,image,dirs}.rs`, `crates/darkshot-macos/src/*` (Task 103), `crates/darkshot-windows/src/{tray,lang,capture,windows_main}.rs` (cách Windows nối menu, cổng chụp, ngôn ngữ, thông báo), `scripts/check.sh`.

## Files

Tạo `crates/darkshot-macos/src/{app,tray,notify,capture,lang}.rs`; sửa `main.rs`, `Cargo.toml` (thêm `objc2-core-graphics`, `objc2-screen-capture-kit`, `objc2-user-notifications`, `block2`, ghim `=`); sửa `crates/darkshot-core/src/i18n.rs` (chuỗi mới, đủ hai bản); tạo `scripts/bundle-macos.sh`, `scripts/demo-macos.sh`, `docs/testing/macos-local-signing.md`; sửa `.gitignore` nếu bản dựng ra thư mục mới (dùng `target/macos`, đã nằm trong `/target`).

## Phạm vi

1. **`scripts/bundle-macos.sh [--release]`** (quyết định 137): build `darkshot-macos`, dựng `target/macos/darkshot.app/Contents/{MacOS/darkshot,Info.plist}`. `Info.plist`: `CFBundleIdentifier` = `app.darkshot.macos`, `CFBundleName` `darkshot`, `CFBundleExecutable` `darkshot`, `CFBundleShortVersionString` lấy từ `Cargo.toml`, `LSUIElement` = `true` (không biểu tượng Dock), `LSMinimumSystemVersion` = `14.0`, `NSScreenCaptureUsageDescription` (chuỗi giải thích). Ký bằng `codesign --force --sign "${DARKSHOT_SIGN_IDENTITY:--}" --identifier app.darkshot.macos`; nếu danh tính là `-` (ad-hoc) in cảnh báo rõ rằng quyền Screen Recording và Keychain sẽ bị hỏi lại sau mỗi lần build. Không đưa danh tính hay chứng chỉ vào repo.
2. **`docs/testing/macos-local-signing.md`**: hướng dẫn chủ dự án tạo chứng chỉ tự ký "Code Signing" cục bộ (Keychain Access → Certificate Assistant), dùng qua `DARKSHOT_SIGN_IDENTITY="<tên chứng chỉ>"`, kiểm bằng `codesign -dv --verbose=4`, và cách đặt lại quyền (`tccutil reset ScreenCapture app.darkshot.macos`). Không có chứng chỉ thật trong tài liệu.
3. **Vỏ ứng dụng (`app.rs`)**: `NSApplication` chính sách `accessory`, vòng lặp chính, một phiên bản duy nhất (đã có bản chạy cùng bundle id thì thoát), Quit chấm dứt sạch.
4. **Thanh menu (`tray.rs`)**: `NSStatusItem` (biểu tượng mẫu `camera.viewfinder` hoặc tương đương) và `NSMenu` dựng từ `tray_logic::menu_layout(...)`; nhãn lấy qua `Msg` của core theo `MenuCommand` (xem `windows/tray.rs`). Chỉ các lệnh đã làm (Chụp, Thoát) được bật; lệnh chưa làm (Lặp lại, Lịch sử, Tùy chọn, Giới thiệu, Trợ giúp, Chụp từng bước, …) **bị loại khỏi menu** bằng một hàm thuần `implemented(MenuCommand) -> bool` có test, để Task sau chỉ việc bật.
5. **Cổng chụp**: dùng `capture_gate::gate_step` (menu chọn "Chụp" → chờ menu tắt hẳn bằng `NSTimer` ngắn như Windows → chụp); không bao giờ hai lượt chụp cùng lúc.
6. **Chụp (`capture.rs`)**: `SCShareableContent` + `SCScreenshotManager.captureImage(contentFilter:configuration:)` cho từng màn hình (`NSScreen`), kích thước cấu hình = pixel vật lý, `showsCursor` theo `Config.include_cursor` (mặc định của core). Kết quả:
```rust
pub struct DisplayShot { pub image: Image /* BGRA, từ trên xuống, pixel vật lý */,
                         pub frame_pt: RectF /* toạ độ toàn cục gốc trên trái, đơn vị điểm */,
                         pub scale: f64, pub display_id: u32 }
pub fn capture_displays(include_cursor: bool) -> Result<Vec<DisplayShot>, CaptureError>;
```
   Chuyển `CGImage` sang BGRA bằng một hàm thuần có test trên bộ đệm byte (đúng thứ tự kênh, alpha trước-nhân). Hàm chụp **không chạy trên luồng chính**; kết quả đưa về luồng chính bằng hàng đợi chính. Lỗi có kiểu (`PermissionDenied`, `NoDisplay`, `Failed(String)`), không `panic`.
7. **Quyền**: `CGPreflightScreenCaptureAccess`; chưa có thì `CGRequestScreenCaptureAccess` một lần và hiện thông báo hướng dẫn mở System Settings → Privacy & Security → Screen Recording (chuỗi mới `Msg::…` trong `i18n.rs`, đủ Việt và English). Không chụp ra ảnh rỗng im lặng.
8. **Thông báo (`notify.rs`)**: `UNUserNotificationCenter` triển khai `darkshot_core::actions::Notifier`; xin quyền gửi thông báo ở lần đầu; chạy ngoài bundle hoặc bị từ chối thì ghi một dòng ra `stderr` và không crash. Luật hiển thị theo `actions::notice_visible` (như Windows).
9. **Ngôn ngữ (`lang.rs`)**: `pub fn lang_from_locale(tag: &str) -> Lang` (thuần, có test: `vi`, `vi-VN` → Việt; còn lại → English) và hàm đọc `NSLocale.preferredLanguages` khi chưa có tệp cấu hình hoặc tệp hỏng; có `Config` thì dùng ngôn ngữ trong cấu hình.
10. **Keychain theo hồ sơ (quyết định 138, từ rà soát Task 103)**: `KeychainStore::new()` hiện luôn dùng service `darkshot`, bỏ qua `DARKSHOT_PROFILE_DIR`; chạy demo cách ly rồi lưu khóa thử sẽ GHI ĐÈ khóa R2 thật. Thêm `KeychainStore::for_profile(profile: Option<&Path>)`: không có hồ sơ cách ly thì service `darkshot`; có thì service `darkshot-sandbox-<băm FNV-1a 64 bit của đường dẫn, chữ thường>` (hàm băm thuần, có test; cùng đường dẫn cho cùng service, đường dẫn khác cho service khác, không bao giờ trùng `darkshot`). Dùng `for_profile(paths::profile_override().as_deref())` ở mục 11 bên dưới và ở mọi nơi tạo `KeychainStore` trong ứng dụng. `scripts/demo-macos.sh` khi dọn phải xóa mục Keychain của hồ sơ cách ly đó (`security delete-generic-password -s <service> -a r2-secret-access-key`) và in lệnh này; không bao giờ xóa mục `darkshot` thật.
11. **Nối cấu hình**: `darkshot_core::config::load(&paths::config_path(), &KeychainStore::for_profile(paths::profile_override().as_deref()), &paths::default_save_dir(), os_lang)` với `os_lang` từ `lang.rs` (chữ ký 4 tham số, xem `config.rs`); xử lý `Loaded` như `windows_main` (thiếu R2 → thông báo; chưa có cửa sổ Tùy chọn nên chỉ thông báo, không mở gì).
12. **Đường chụp tạm (xóa ở Task 105)**: sau khi chụp, lấy màn hình chứa con trỏ, chạy `actions::run_action(ctx, Action::Save, &image)` trên luồng nền rồi báo kết quả bằng thông báo. `Ctx.clipboard` dùng `PasteboardClipboard` (Task 103), `Ctx.uploader` = `None`.
13. **`scripts/demo-macos.sh`**: build bundle bằng `bundle-macos.sh`, đặt `DARKSHOT_PROFILE_DIR` về một thư mục tạm riêng (cách ly: không đụng cấu hình thật), chạy `open -n` bản dựng, in hướng dẫn thử và cách dọn.

## Chuỗi i18n mới (đủ Việt / English)

Do builder đề xuất tên `Msg` và nội dung, tối thiểu: tiêu đề và nội dung thông báo "cần quyền Ghi màn hình" kèm đường dẫn menu System Settings; "không tìm thấy màn hình để chụp"; nhãn menu nếu `Msg` hiện có chưa đủ. Tuân quy tắc `messages!`: thiếu một bản là lỗi biên dịch.

## Kiểm thử (viết trước)

- Thuần (chạy tự động): `for_profile`/băm service (cùng đường dẫn → cùng service; khác đường dẫn → khác; `None` → `darkshot`; không bao giờ trùng `darkshot` khi có hồ sơ), `implemented`, `lang_from_locale`, chuyển `CGImage`→BGRA trên bộ đệm byte tổng hợp, ánh xạ `MenuCommand` → `Msg`, dựng nội dung `Info.plist` nếu viết bằng hàm (hoặc kiểm bằng `plutil -lint` trong script), `DisplayShot` từ khung màn hình (dùng `coords`).
- `#[ignore]` chạy tay: chụp thật từng màn hình (kích thước ảnh = `frame_pt × scale`), kiểm quyền.
- `scripts/check.sh` phải qua; `bash -n scripts/bundle-macos.sh scripts/demo-macos.sh`; script có `set -eu` và không in danh tính ký.

## QA tay (chủ dự án)

1. Tạo chứng chỉ ký cục bộ theo `docs/testing/macos-local-signing.md`, `export DARKSHOT_SIGN_IDENTITY=…`, chạy `scripts/demo-macos.sh`.
2. Biểu tượng hiện trên thanh menu, không có biểu tượng Dock. Menu chỉ có các mục đã làm; chữ đúng ngôn ngữ hệ thống.
3. Lần đầu "Chụp": hệ thống hỏi quyền Screen Recording; sau khi cấp (có thể cần mở lại ứng dụng) chụp được; thiếu quyền thì có thông báo hướng dẫn, không có ảnh rỗng.
4. Ảnh lưu có kích thước pixel đúng màn hình (Retina: gấp đôi điểm); con trỏ có/không theo cấu hình mặc định.
5. Build lại cùng chứng chỉ ký **không** làm hỏi lại quyền; build ad-hoc thì có cảnh báo của script (đã ghi trong tài liệu).
6. Khởi động lần thứ hai khi đang chạy: không mở bản thứ hai. Thoát từ menu sạch (không treo).
7. Keychain: build lại bằng cùng chứng chỉ rồi mở, xem có hỏi lại quyền không. Thử bấm **Từ chối** để thấy luồng "mất khóa" hiện ra mà ứng dụng không crash. Chạy demo cách ly, nhập khóa thử và lưu (khi đã có cửa sổ Tùy chọn ở M4b), rồi xác nhận khóa R2 thật (service `darkshot`) KHÔNG đổi và mục `darkshot-sandbox-…` bị dọn khi kết thúc demo.

## Review Focus

1. Không chặn luồng chính; kết quả chụp về đúng luồng; không gọi AppKit từ luồng nền; cổng chụp không cho hai lượt chồng nhau.
2. Quyền: không chụp ảnh rỗng, thông báo đủ hai ngôn ngữ, không gọi `CGRequestScreenCaptureAccess` lặp lại gây phiền.
3. `DisplayShot` đúng kích thước và hệ tọa độ; BGRA đúng kênh; con trỏ theo cấu hình.
4. `bundle-macos.sh`: không rò danh tính hay chứng chỉ, không `rm -rf` ngoài `target/macos`, `plist` hợp lệ, ký đúng.
5. Khóa R2 chỉ qua Keychain; không lọt vào log hay thông báo; hồ sơ cách ly KHÔNG bao giờ dùng service `darkshot` thật (mục 10).
6. Job `windows` vẫn xanh (crate Mac ở dạng khung); không dùng `cfg(windows)` trong core.

## Global Constraints

Như mục cùng tên của `docs/superpowers/plans/2026-11-21-macos-port.md` (chép đủ ở đây):
- `darkshot-core` không phụ thuộc crate `windows`, `native-tls`, `schannel`, `rustls-platform-verifier`, không `cfg` theo hệ điều hành. Sửa core chỉ để thêm chuỗi i18n (đủ hai bản Việt và English) hoặc một trừu tượng thật sự cần; mọi chữ người dùng thấy qua `darkshot_core::i18n`, không viết cứng.
- **Mọi phụ thuộc theo macOS nằm dưới `[target.'cfg(target_os = "macos")'.dependencies]` và mọi mã Objective-C/FFI dưới `#[cfg(target_os = "macos")]`**: job `windows` của CI vẫn biên dịch crate `darkshot-macos` ở dạng khung và không được hỏng. Ghim phiên bản chính xác (`=x.y.z`) cho mọi crate `objc2*`, `block2`, `dispatch2`, `security-framework`, `libc`.
- Mỗi API Objective-C/C bọc trong module nhỏ; mỗi khối `unsafe` có `// SAFETY:` nêu bất biến. Không `unwrap`/`expect`/`panic!` trên đường chạy thật (`panic = "abort"`); mọi lỗi là `Result` hoặc thông báo cho người dùng.
- Không bao giờ chặn luồng chính bằng việc lâu (chụp màn hình, mã hóa PNG, ghi tệp, mạng); việc nền báo về luồng chính qua hàng đợi chính. Không gọi AppKit từ luồng nền.
- Khóa R2 chỉ qua `SecretStore`; không vào log, thông báo lỗi, tệp cấu hình, tham số dòng lệnh, kết quả test. Test không dùng khóa thật hay tên Keychain thật của ứng dụng. Không đọc, ghi hay in khóa R2 của chủ dự án.
- Repo công khai: không đưa chứng chỉ, Team ID, profile, đường dẫn cá nhân hay ảnh chụp màn hình thật vào repo.
- Logic thuần (tọa độ, ánh xạ, bố cục, luật) viết trước bằng test (TDD) và nằm ở nơi test chạy được trên máy không có GUI; phần cần GUI kiểm bằng test `#[ignore]` chạy tay và danh sách QA tay bên dưới. **QA không giả lập chuột/phím thật** khi chủ dự án dùng máy (quyết định 126): agent không tự bấm chuột, không tự cấp quyền hệ thống.
- Chạy `rustfmt`/`cargo fmt --all`, `./scripts/check.sh` phải in `ALL CHECKS PASSED`. Không sửa `memory-bank/` (phiên điều phối làm). Commit kết thúc bằng dòng `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; không tự đẩy.

## Phụ thuộc

Task 103.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
