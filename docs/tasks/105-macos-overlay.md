# Task 105: lớp phủ chọn vùng macOS (đợt M, bước M3c)

**Agent:** `builder` · **Model:** opus · **Nhánh:** `task/105-macos-overlay` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (fable) · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml` (Windows chỉ biên dịch phần khung của crate Mac) + QA tay của chủ dự án (dưới)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` (mục 3, 4, 5 và 9). Quyết định 127–137 (đặc biệt 134–137). Tiền lệ cách làm: `docs/tasks/101-core-shared-logic.md`.

## Mục tiêu

Thay đường chụp tạm của Task 104 bằng lớp phủ chọn vùng thật: mỗi màn hình một cửa sổ phủ ảnh vừa chụp, kéo chọn vùng bằng `Selection` của core, thanh **Lưu / Chép / Hủy**, kết quả đi qua `actions::run_action`. **Chưa có** công cụ vẽ (M5), tải lên (M4a), bắt cửa sổ, lặp lại lần chụp trước.

## Cần đọc

`crates/darkshot-core/src/{geometry,overlay_layout,capture_gate,actions,i18n,image}.rs` (`Selection`, `place_toolbar`, `monitor_containing`, hằng `BUTTON`/`GAP`/`HANDLE_*`, `bar_layout`, `Image::crop`), `crates/darkshot-windows/src/overlay.rs` (chỉ các phần: máy trạng thái kéo thả và bàn phím cho Lưu/Chép/Hủy/Esc/Enter; cách vẽ lớp mờ, khung, tay nắm, nhãn kích thước; bỏ qua mọi thứ về công cụ vẽ), `crates/darkshot-macos/src/*` (Task 103–104).

## Chuỗi i18n mới (đủ Việt / English)

Ba chuỗi gợi ý nút kiểu Mac (Lưu ⌘S, Chép ⌘C, Hủy Esc) nêu ở phạm vi mục 5; tên `Msg` do builder đề xuất, tuân quy tắc `messages!`.

## Files

Tạo `crates/darkshot-macos/src/{overlay,overlay_view}.rs` (và module phụ nếu hợp lý: `overlay_input.rs` cho phần thuần); sửa `app.rs` (nối lớp phủ vào cổng chụp, xóa đường chụp tạm của Task 104), `Cargo.toml` nếu cần feature `objc2-app-kit`; sửa `i18n.rs` nếu cần chuỗi (đủ hai bản); cập nhật `scripts/demo-macos.sh` nếu cần.

## Phạm vi

1. **Cửa sổ**: một `NSWindow` không viền cho mỗi `NSScreen` (lớp con cho `canBecomeKey = true`), mức cửa sổ trên menu bar (`CGShieldingWindowLevel` hoặc tương đương), `collectionBehavior` phủ mọi Space và Space toàn màn hình (`canJoinAllSpaces | fullScreenAuxiliary | stationary`), phủ đúng `NSScreen.frame` (kể cả vùng khoét tai thỏ), nền trong suốt.
2. **View** (lớp con `NSView` định nghĩa bằng `objc2::define_class!`): `isFlipped = true` (gốc trên trái như core); vẽ ảnh `DisplayShot` kín view, phủ mờ (đen ~45%), khoét vùng chọn để hiện ảnh sáng, viền 1 px, tám tay nắm, nhãn kích thước **pixel thật** (`W × H`, đúng cỡ ảnh sẽ lưu) gần vùng chọn. Vẽ bằng Core Graphics; vẽ trong không gian pixel của màn hình đó (nhân `scale`) để khớp `Selection` và bố cục của core.
3. **Chuột**: `mouseDown/Dragged/Up` → đổi điểm sang pixel bằng `coords` → `Selection` của core (`press`, `drag_to`, `release`, bán kính tay nắm = `HANDLE_HIT` × `scale`). Chọn mới, di chuyển, đổi cỡ bằng tay nắm như Windows. Nhấn chuột ở màn hình khác thì màn hình đó thành màn hình đang chọn (vùng chọn cũ ở màn hình kia bị bỏ); **vùng chọn không vắt qua màn hình** (quyết định 135). Con trỏ chữ thập; con trỏ đổi cỡ trên tay nắm là tùy chọn.
4. **Bàn phím**: `Esc` hủy; các phím cho Lưu/Chép theo đúng cách Windows làm (builder đọc `overlay.rs` và nêu bảng phím trong báo cáo; `Enter`/`Cmd+C`/`Cmd+S` là gợi ý nếu Windows không có tương đương). Cửa sổ phải nhận phím ngay khi hiện (không cần bấm trước).
5. **Thanh hành động**: sau khi nhả chuột có vùng hợp lệ, hiện thanh ba nút **Lưu, Chép, Hủy** cạnh vùng chọn bằng `geometry::place_toolbar` (kích thước và khoảng cách từ hằng của `overlay_layout`, nhân `scale`), nhãn/gợi ý: các `Msg::OverlaySave/Copy/Cancel` của core ghi phím tắt kiểu Windows (`Ctrl+S`, `Ctrl+C`), **không đúng trên Mac**; thêm chuỗi riêng cho Mac (ví dụ `OverlaySaveMac`: "Lưu vào máy (⌘S)" / "Save to disk (⌘S)", tương tự Chép và Hủy `Esc`) vào `i18n.rs` đủ hai bản và dùng chúng trên Mac; không đổi chuỗi Windows. Kích thước nhỏ nhất của vùng chọn theo luật của core. Không có nút Tải lên, Hoàn tác, công cụ vẽ.
6. **Kết quả**: bấm nút hoặc phím → đóng mọi cửa sổ phủ → `Image::crop` đúng vùng pixel của `DisplayShot.image` → `actions::run_action(ctx, action, &cropped)` **trên luồng nền** (không mã hóa PNG trên luồng chính) → thông báo kết quả qua `Notifier` đã có. `Clipboard` là `PasteboardClipboard`; `uploader` = `None`. `Action::Cancel` không làm gì.
7. **Cổng chụp**: đóng lớp phủ (mọi đường: hành động, `Esc`, đổi cấu hình màn hình, mất quyền) phát `GateEvent::OverlayClosed`; không bao giờ để lại cửa sổ phủ mồ côi hay khóa cổng. Thay đổi bố cục màn hình (cắm/rút) trong lúc đang chọn thì hủy sạch.
8. **Xóa đường chụp tạm** của Task 104 (lưu cả màn hình), không để mã chết.

## Kiểm thử (viết trước)

- Thuần (tự động, nằm ở module không cần GUI): ánh xạ điểm→pixel của sự kiện chuột (kể cả màn hình phụ có gốc âm, Retina 2x); `selection_to_pixels` + `Image::crop` cho ra đúng kích thước (vùng 200×100 điểm ở 2x → 400×200 pixel); vị trí thanh hành động từ `place_toolbar` luôn nằm trong màn hình ở bốn góc và khi vùng chọn sát mép; ánh xạ phím → hành động; nhãn kích thước bằng pixel thật; chuyển trạng thái "màn hình đang chọn" khi bấm sang màn hình khác.
- `#[ignore]` chạy tay: dựng một lớp phủ từ `DisplayShot` tổng hợp (ảnh gradient) và hiện lên, đóng bằng `Esc`.
- `./scripts/check.sh` phải qua.

## QA tay (chủ dự án)

1. `scripts/demo-macos.sh` → Chụp: ảnh màn hình hiện mờ, kéo chọn vùng, nhãn `W × H` đúng; sau khi nhả chuột hiện thanh Lưu/Chép/Hủy cạnh vùng.
2. Di chuyển và đổi cỡ vùng bằng tám tay nắm; kéo sát mép và sát tai thỏ/menu bar, thanh vẫn nằm trong màn hình.
3. Lưu: có thông báo, tệp PNG trong thư mục lưu có kích thước **pixel** đúng (vùng 200×100 điểm ở màn hình Retina ra 400×200 px); mở bằng Preview thấy đúng nội dung vùng chọn, không lệch một dòng pixel, không bị lật.
4. Chép: dán vào Preview/ứng dụng khác ra đúng ảnh. Hủy và `Esc`: đóng sạch, không để lại cửa sổ; chụp lại được ngay.
5. Nếu có màn hình thứ hai (hoặc Sidecar): lớp phủ phủ cả hai, chọn ở màn hình nào thì lưu đúng màn hình đó; không thể kéo vùng sang màn hình kia.
6. Đang ở ứng dụng toàn màn hình (Space riêng): lớp phủ vẫn hiện trên cùng và nhận phím. Chuyển ứng dụng bằng `Cmd+Tab` khi đang chọn không để lại lớp phủ.
7. Đổi cấu hình màn hình (rút hoặc cắm màn hình, đổi độ phân giải) khi đang chọn: lớp phủ hủy sạch, không treo.

## Review Focus

1. Tọa độ: lật trục Y, `scale`, màn hình phụ gốc âm; vùng crop chính xác đến từng pixel; không lật ảnh.
2. Vòng đời cửa sổ: mọi đường đóng đều dọn cửa sổ và phát `OverlayClosed`; không rò đối tượng Objective-C (retain/release đúng, `define_class!` đúng); không gọi AppKit từ luồng nền; `run_action` không chạy trên luồng chính.
3. Bàn phím và tiêu điểm: cửa sổ borderless nhận phím; `Esc` luôn hủy được; không kẹt lớp phủ che toàn màn hình (đây là lỗi nặng: luôn phải có đường thoát).
4. `Selection` của core là nguồn duy nhất cho trạng thái kéo thả; không tự tính lại.
5. Không sót mã của đường chụp tạm Task 104; không mã chết.
6. Job `windows` vẫn xanh; không đụng core ngoài chuỗi i18n.

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

Task 104.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
