# Task 107: cửa sổ Tùy chọn macOS (đợt M, bước M4b)

**Agent:** `builder` · **Model:** opus · **Nhánh:** `task/107-macos-options` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus; fable khi có credits) · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml` + QA tay của chủ dự án (dưới)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` (mục 3–5, 9). Quyết định 127–141 (đặc biệt 131, 135, 136, 138–141). Tiền lệ đã QA thật: Task 103, 104, 105.

## Mục tiêu

Cửa sổ Tùy chọn bằng AppKit thuần, giao diện tối, dùng đúng logic biểu mẫu của core (`settings_form`: `Form`, `apply_form`, `Tab`, `Field`, `FormError`, `tab_of`, `initial_tab`, `tabs_fit`, `quality_enabled`) và bắt phím của core (`hotkey_keys::{CaptureState, KeyEv, Capture}`). Sau gói này chủ dự án nhập được thông tin R2 (khóa vào Keychain) và đổi phím tắt, thư mục lưu, ngôn ngữ ngay trong ứng dụng.

## Cần đọc

`crates/darkshot-core/src/{settings_form,hotkey_keys,config,app_flow,app_state,i18n,upload}.rs` (đặc biệt `apply_form(current, form, lang) -> Result<Config, FormError>`, `Form`, `R2Uploader::test_connection`, mọi `Msg` có tiền tố `Settings`/`Tab`), `crates/darkshot-windows/src/settings.rs` (bố cục 4 tab, thứ tự điều khiển, cách mở, nút Kiểm tra kết nối, điều khiển bắt phím, luồng lưu: `config::save`, `lang::set`, đăng ký lại phím), `crates/darkshot-macos/src/{app,tray,secret,lang,paths,keymap,hotkey}.rs`.

## Files

Tạo `crates/darkshot-macos/src/{options,options_view,hotkey_field}.rs` (tách phần thuần vào `options_model.rs` nếu hợp lý); sửa `app.rs` (mở Tùy chọn từ menu và khi cần: `needs_settings_on_start`, `OpenReason`), `tray.rs` (bật mục Tùy chọn, hàm `implemented`), `lang.rs`, `Cargo.toml` (feature `objc2-app-kit` cho `NSTabView`, `NSTextField`, `NSSecureTextField`, `NSButton`, `NSPopUpButton`, `NSSlider`, `NSOpenPanel`, `NSAlert`, `NSAppearance`; ghim `=`); `i18n.rs` chỉ thêm chuỗi (đủ hai bản).

## Phạm vi

1. **Cửa sổ**: `NSWindow` (không phải lớp phủ), tiêu đề `Msg` có sẵn, `NSAppearance(.darkAqua)` ép cho cả cửa sổ (giao diện luôn tối), `NSTabView` với đúng 4 tab của `Tab::ALL` theo thứ tự (Chung, Phím tắt, Định dạng, R2) và nhãn `Tab::label`; một cửa sổ duy nhất (mở lần hai thì đưa cửa sổ cũ ra trước); OK/Hủy. Mở tab theo `initial_tab(reason)`.
2. **Điều khiển theo từng `Field`** (đúng các trường mà `Form` có; xem Windows `build_general/build_hotkeys/build_format/build_r2` để biết đủ): thư mục lưu (ô chữ + nút Chọn bằng `NSOpenPanel` thư mục), ngôn ngữ, thời hạn link, bật/tắt thông báo thành công, giữ vị trí vùng, chèn con trỏ, tự khởi động (hiển thị nhưng `SMAppService` làm ở gói sau: ô bị mờ kèm ghi chú nếu chưa làm), kiểm tra cập nhật (nếu `Form` có), lịch sử (nếu có), bắt cửa sổ (nếu có): chỉ hiện điều khiển mà tính năng Mac đã làm; điều khiển của tính năng chưa làm thì ẩn (liệt kê lựa chọn trong báo cáo); định dạng PNG/JPEG và chất lượng (`quality_enabled`); bốn dòng phím tắt (bật/tắt + nút đổi phím); R2: Account ID, Access Key ID, Secret Access Key (ô mật khẩu, giữ khóa cũ khi để trống theo `apply_form`), Bucket, URL công khai, nút **Kiểm tra kết nối** chạy `R2Uploader::test_connection` trên luồng nền và báo kết quả về luồng chính (không chặn giao diện; không bao giờ in khóa).
3. **Ô bắt phím (`hotkey_field.rs`)**: khi nhận tiêu điểm và người dùng bấm tổ hợp, đổi mã phím macOS → VK bằng `keymap::mac_to_vk`, chuyển cho `CaptureState`/`capture_hotkey` của core (Down/Up, các luật phím hợp lệ, Alt+F4-tương đương không cần), hiển thị bằng `hotkey_keys::describe` (hoặc nhãn Mac `⌃⇧4` nếu builder thêm hàm hiển thị riêng Mac, có test). Không bắt được phím thì báo `NeedModifier` như Windows. Esc hủy việc bắt.
4. **Lưu (OK)**: đọc điều khiển → `Form` → `apply_form(&current, &form, lang)`; lỗi → `NSAlert` với `FormError.message`, chuyển đến tab chứa ô sai (`tab_of(field)`), đưa tiêu điểm vào ô đó, KHÔNG đóng cửa sổ; thành công → `config::save(path, &cfg, &KeychainStore::for_profile(..))` (khóa mới vào Keychain; lỗi lưu báo cho người dùng, giữ cửa sổ mở), cập nhật cấu hình đang chạy, `lang::set`, đăng ký lại phím tắt bằng `hotkey::apply` (phím bận → thông báo), dựng lại menu nếu ngôn ngữ đổi. Hủy/đóng cửa sổ: bỏ thay đổi, KHÔNG ghi tệp.
5. **Mở tự động**: khi `app_state::needs_settings_on_start` hoặc khi lưu không thể tải lên vì thiếu R2 (`OpenReason::NeedsR2`): mở thẳng tab R2. Hộp thoại mở theo `app_flow::merge_reason` khi có hai yêu cầu liên tiếp.
6. **Không chặn luồng chính**: kiểm tra kết nối ở luồng nền; thư mục chọn bằng `NSOpenPanel` bất đồng bộ (`beginSheetModal`/`begin`), không `runModal` làm treo vòng lặp.
7. **Bố cục**: dùng Auto Layout (`NSStackView`/ràng buộc) hoặc tính khung từ hằng; chữ không bị cắt ở cả hai ngôn ngữ (dùng chuỗi dài nhất English/Việt để kiểm); cửa sổ không đổi kích thước được hoặc co giãn hợp lý; hoạt động với tăng cỡ chữ hệ thống.

## Chuỗi i18n

Chuỗi core đã đủ cho phần lớn (nhãn tab, trường, lỗi, nút); thêm chỉ những gì thiếu cho Mac (ví dụ nhãn nút Chọn thư mục kiểu Mac nếu khác), đủ hai bản.

## Kiểm thử (viết trước)

- Thuần (`options_model.rs`): đọc/ghi giữa trạng thái điều khiển và `Form` (khứ hồi với `Config` đầu vào: mở rồi OK không đổi gì thì `apply_form` trả cấu hình tương đương), khóa bí mật để trống giữ khóa cũ, tab chứa lỗi, `initial_tab`, thứ tự bắt phím qua `keymap`, nhãn phím Mac.
- Test không đụng khóa thật: dùng `KeyBackend` giả đã có.
- `#[ignore]`: mở cửa sổ tổng hợp rồi đóng (cờ ẩn như `--overlay-selftest`, đặt tên `--options-selftest`, chỉ hiện cửa sổ với `Config` mẫu, không đọc cấu hình thật, không Keychain thật).

## QA tay (chủ dự án)

1. Menu → Tùy chọn: cửa sổ tối, 4 tab, chữ đúng cả Việt và English (đổi ngôn ngữ rồi mở lại).
2. Tab R2: nhập thông tin R2 của riêng bạn (CHỈ bạn nhập, tôi và agent không bao giờ thấy khóa), bấm Kiểm tra kết nối: báo thành công hoặc lỗi dễ hiểu; giao diện không treo. Lưu rồi mở lại: khóa bí mật không hiện lại (ô trống = giữ khóa cũ); khóa nằm trong Keychain (Keychain Access, service `darkshot` hoặc `darkshot-sandbox-…` nếu chạy demo cách ly).
3. Nhập sai (URL công khai hỏng, trùng phím tắt): hiện lỗi đúng, chuyển đúng tab, không đóng cửa sổ.
4. Đổi phím tắt chụp vùng bằng ô bắt phím rồi OK: phím mới có hiệu lực ngay, phím cũ hết; phím bị chiếm báo rõ.
5. Hủy: không thay đổi gì, tệp cấu hình không bị ghi.
6. Chưa có R2 mà chạy ứng dụng lần đầu: cửa sổ mở thẳng tab R2.

## Review Focus

1. **Khóa bí mật**: không vào log, `NSAlert`, tên điều khiển, `Debug`, tham số; ô mật khẩu là `NSSecureTextField`; để trống giữ khóa cũ; khóa chỉ đi vào Keychain qua `config::save`; Hủy không ghi gì.
2. Dùng ĐÚNG `apply_form` và `Form` của core (không tự kiểm lại luật); lỗi và thứ tự lỗi như Windows.
3. Luồng: kiểm tra kết nối và mọi việc lâu ở luồng nền; AppKit chỉ ở luồng chính; không `runModal`.
4. Vòng đời cửa sổ/đối tượng Objective-C, một cửa sổ duy nhất, không rò khi mở đóng nhiều lần.
5. Bố cục không cắt chữ ở cả hai ngôn ngữ; giao diện tối đúng.
6. Đăng ký lại phím tắt và đổi ngôn ngữ có hiệu lực ngay.

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

Task 106.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
