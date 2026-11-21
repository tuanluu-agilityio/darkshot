# Task 106: phím tắt toàn cục macOS (đợt M, bước M4a)

**Agent:** `builder` · **Model:** opus · **Nhánh:** `task/106-macos-hotkeys` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus; fable khi có credits) · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml` + QA tay của chủ dự án (dưới)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` (mục 3–5, 9). Quyết định 127–141 (đặc biệt 131, 135, 136, 138–141). Tiền lệ đã QA thật: Task 103, 104, 105.

## Mục tiêu

Chụp bằng phím tắt toàn cục (không cần mở menu): vùng chọn (lớp phủ), toàn màn hình lưu ngay, toàn màn hình tải lên (xem Task 108 cho phần tải lên). Phím lưu trong cấu hình vẫn là mã VK của Windows (quyết định 131); lớp Mac chỉ thêm bảng đổi mã phím macOS ↔ VK và bộ mặc định riêng cho Mac (quyết định 139).

## Cần đọc

`crates/darkshot-core/src/{config,hotkey_keys,capture_gate,app_flow,notices,actions,i18n}.rs` (`Hotkey`, `Hotkeys`, `HotkeyRole`, `hotkey_keys::{id_of,role_of,describe,taken_combos}`, `capture_gate::{gate_step,GateEvent::Hotkey,kind_of,next_kind}`, `notices::hotkeys_taken_notice`), `crates/darkshot-windows/src/{hotkey,windows_main}.rs` (cách Windows đăng ký, xử lý `WM_HOTKEY`, phím bận, nhận diện loại chụp), `crates/darkshot-macos/src/{app,tray,overlay,capture}.rs`.

## Files

Tạo `crates/darkshot-macos/src/{keymap,hotkey}.rs`; sửa `app.rs` (nối phím tắt vào cổng chụp), `main.rs`, `tray.rs` nếu cần (không thêm mục menu mới); sửa `Cargo.toml` nếu cần feature/framework (`Carbon`); sửa `i18n.rs` chỉ để thêm chuỗi (đủ hai bản).

## Phạm vi

1. **`keymap.rs` (thuần, có test)**: bảng đổi mã VK của Windows ↔ mã phím ảo macOS (`kVK_*`): chữ A–Z, số 0–9, F1–F20 (F13–F20 có trên Mac), mũi tên, Space, Return, Tab, Esc, Delete, Home/End/PageUp/PageDown; PrintScreen/ScrollLock/Pause (VK 0x2C/0x91/0x13) ↔ F13/F14/F15 (bàn phím Apple mở rộng). `modifiers`: `ctrl→Control`, `alt→Option`, `shift→Shift`, `win→Command`. Mã chưa hỗ trợ trả `None`. Hàm khứ hồi `vk_to_mac`/`mac_to_vk` thống nhất trên mọi mã được hỗ trợ.
2. **Bộ mặc định Mac (quyết định 139)**: `pub fn mac_default_hotkeys() -> Hotkeys` (đặt trong `hotkey.rs` hoặc `keymap.rs`), dùng cho lần chạy đầu khi chưa có tệp cấu hình (không đổi định dạng tệp cấu hình, không sửa `Config::defaults` của core). Đề xuất: chụp vùng `Control+Shift+4` bật; lưu toàn màn hình `Control+Shift+3` tắt; tải lên toàn màn hình `Control+Option+3` tắt; lặp lại lần chụp trước tắt/ẩn. Builder xác nhận không trùng phím tắt hệ thống mặc định của macOS (⌘⇧3/4/5 là của hệ thống) và nêu bảng cuối cùng trong báo cáo. Hàm thuần, có test.
3. **`hotkey.rs` (FFI Carbon, mỏng)**: đăng ký bằng `RegisterEventHotKey` + `InstallApplicationEventHandler` (không cần quyền Accessibility, không cần cửa sổ), huỷ bằng `UnregisterEventHotKey`; mỗi vai trò một ID từ `hotkey_keys::id_of`; trả về danh sách vai trò đăng ký THẤT BẠI (phím đã bị chiếm) để dùng với `notices::hotkeys_taken_notice`. Khai báo `extern "C"` tối thiểu thay vì thêm crate nếu khả thi (nêu lựa chọn và lý do); nếu dùng crate thì ghim `=`. Carbon đã cũ nhưng vẫn là cách duy nhất không cần quyền; ghi chú điều này.
4. **Nối vào ứng dụng**: sự kiện phím tắt chạy ở luồng chính → `capture_gate::gate_step(.., GateEvent::Hotkey)` như menu; `Region` mở lớp phủ (đường sẵn có); `SaveFull` chụp màn hình chứa con trỏ (hàm `pick_display` sẵn có) và `run_action(Action::Save)` trên luồng nền; `UploadFull` giữ chỗ cho Task 108 (khi chưa có tải lên thì phát thông báo "chưa cấu hình" qua đường sẵn có của core, không crash); `RepeatLast` không làm trong gói này (ẩn khỏi mọi giao diện, ghi chú vì quyết định 135 làm tọa độ lần chụp trước khác Windows). Phím nhấn khi đang có lớp phủ hoặc cổng bận bị bỏ qua đúng theo `gate_step`.
5. **Đăng ký lúc khởi động** theo `Config.hotkeys` đã nạp (hoặc bộ mặc định Mac khi chưa có tệp); phím bận → một thông báo nêu phím nào bị chiếm (core đã có chuỗi). Đăng ký lại khi cấu hình đổi (Task 107 sẽ gọi); cung cấp hàm `apply(&Hotkeys) -> Vec<HotkeyRole>` để 107 dùng.
6. **Không** thêm cửa sổ cài đặt phím tắt ở đây (Task 107).

## Chuỗi i18n

Nếu cần (ví dụ nhãn phím Mac) thêm vào `i18n.rs` đủ hai bản; ưu tiên dùng chuỗi core sẵn có.

## Kiểm thử (viết trước)

- `keymap`: khứ hồi VK↔Mac cho mọi mã được hỗ trợ; PrintScreen→F13; mã lạ → `None`; ánh xạ phím bổ trợ.
- Bộ mặc định Mac: không có vai trò nào dùng PrintScreen (0x2C) trừ khi ánh xạ F13 là chủ ý; ba vai trò khác nhau; không trùng nhau; `describe` ra chuỗi người đọc được.
- Logic chọn hành động theo `GateEvent::Hotkey` đã có test ở core; thêm test cho hàm chuyển `HotkeyRole`→`CaptureKind` nếu viết ở lớp Mac.
- `#[ignore]` (chạy tay: `cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1`): đăng ký một phím thử rồi huỷ đăng ký không lỗi (không bấm phím).

## QA tay (chủ dự án)

1. `./scripts/demo-macos.sh --release`: bấm `Control+Shift+4` ở bất kỳ ứng dụng nào: lớp phủ chọn vùng hiện ra (không cần mở menu).
2. Bật thủ công `Control+Shift+3` (qua tệp cấu hình của hồ sơ demo nếu chưa có cửa sổ Tùy chọn; hướng dẫn trong báo cáo) rồi bấm: ảnh toàn màn hình chứa con trỏ lưu vào thư mục lưu.
3. Phím bị ứng dụng khác chiếm: thông báo nêu đúng phím; ứng dụng không crash.
4. Bấm phím khi lớp phủ đang mở hoặc đang chụp: không mở lớp phủ thứ hai.

## Review Focus

1. Carbon FFI: chữ ký `extern "C"` đúng (EventHotKeyID, EventTypeSpec, OSStatus), vòng đời `EventHotKeyRef` (thả đúng), bộ xử lý sự kiện không panic xuyên ranh giới C, luồng chính.
2. Ánh xạ VK↔Mac đúng giá trị `kVK_*` (đối chiếu `HIToolbox/Events.h` trong SDK CommandLineTools).
3. Mặc định Mac không đè phím hệ thống; không đổi `Config::defaults`/định dạng tệp.
4. Không để cổng chụp kẹt; không mở hai lớp phủ.

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

Task 105 (đã gộp).

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
