# Task 108: tải lên R2 và an toàn khi thoát trên macOS (đợt M, bước M4c)

**Agent:** `builder` · **Model:** opus · **Nhánh:** `task/108-macos-upload` (từ `feat/macos-port`) · **Rà soát:** `reviewer` (opus; fable khi có credits) · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml` + QA tay của chủ dự án (dưới)

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md` (mục 3–5, 9). Quyết định 127–141 (đặc biệt 131, 135, 136, 138–141). Tiền lệ đã QA thật: Task 103, 104, 105.

## Mục tiêu

Nút **Tải lên** trên thanh hành động của lớp phủ, tải ảnh lên R2 bằng `R2Uploader` của core, chép link có hạn vào clipboard, và đảm bảo không mất ảnh khi tải lên dở dang lúc thoát (sổ `PendingUploads` và `save_pending` của core). Phím `UploadFull` của Task 106 dùng cùng đường này.

## Cần đọc

`crates/darkshot-core/src/{upload,actions,app_state,config,overlay_layout,i18n,notices}.rs` (`R2Uploader::new(&R2Config)`, `Action::Upload`, `run_action`, `Outcome::{Uploaded,UploadFailedSaved}`, `PendingUploads`, `save_pending`, `R2Config::is_complete`), `crates/darkshot-windows/src/{app,windows_main}.rs` (`spawn_upload`, `UploadJob`, cách thêm vào sổ, `save_pending_uploads` lúc thoát, thông báo khi chưa cấu hình, thứ tự thoát quyết định 99/124 và `EXIT_BUDGET`), `crates/darkshot-macos/src/{app,overlay,overlay_input,notify,clipboard,secret}.rs`.

## Files

Tạo `crates/darkshot-macos/src/upload.rs`; sửa `overlay_input.rs`/`overlay_view.rs`/`overlay.rs` (nút Tải lên; thanh có bốn nút Tải lên/Lưu/Chép/Hủy theo thứ tự của `bar_cells`/Windows), `app.rs` (tạo `R2Uploader` từ cấu hình, chạy tải lên, thoát), `i18n.rs` chỉ thêm chuỗi Mac cho gợi ý nút Tải lên nếu cần (đủ hai bản).

## Phạm vi

1. **Nút Tải lên** chỉ hiện khi `R2Config::is_complete()` (nêu cách xử lý chưa cấu hình: ẩn nút hay hiện và báo cần thiết lập; theo Windows, đề xuất hiện nút và mở Tùy chọn tab R2 qua `OpenReason::NeedsR2`). Gợi ý nút có phím tắt kiểu Mac nếu Windows ghi phím.
2. **Chạy tải lên** trên luồng riêng (`darkshot-upload`) như Windows: tạo `Ctx` (uploader, clipboard Mac, notifier Mac, ngôn ngữ, định dạng PNG/JPEG và chất lượng theo cấu hình, hạn link), `run_action(Action::Upload)`. Thành công: link nằm trong clipboard và có thông báo (theo `notice_visible`); thất bại: ảnh được lưu dự phòng vào thư mục lưu (core làm) và thông báo nêu rõ, kèm gợi ý mở Tùy chọn nếu do thiếu/sai cấu hình.
3. **Sổ lượt tải lên** (`PendingUploads`): mỗi lượt thêm vào sổ trước khi chạy, rút ra khi xong (`finish`), đúng thứ tự khóa của core; **khi thoát** (`quit` hiện chờ tối đa 3 s): `drain` các lượt còn dở và `save_pending` ghi chúng vào thư mục lưu (core), trong hạn chót chung (`app_flow::EXIT_BUDGET`/`remaining`), không bao giờ làm mất ảnh. Không mạng ở đường thoát.
4. **Phím tải lên toàn màn hình** (`UploadFull`, Task 106) dùng cùng hàm tải lên (chụp màn hình chứa con trỏ rồi tải).
5. **Link có hạn**: dùng `Config.expiry` qua `Ctx.expiry` như Windows; không tự tính. Không đoán được tên đối tượng (core đã làm).
6. **Bảo mật**: khóa R2 chỉ lấy từ cấu hình đã nạp (đã qua Keychain); không vào log, thông báo, stderr; lỗi mạng của `ureq` không chứa khóa (core đã có test, kiểm lại bằng test ở lớp Mac nếu thêm đường mới).

## Kiểm thử (viết trước)

- Thuần: bộ nút của thanh theo trạng thái cấu hình (đủ/thiếu R2), thứ tự nút, hit-test với bốn nút, ánh xạ nút → `Action`.
- Thuần: dựng `Ctx` từ `Config` (định dạng, chất lượng, hạn), chọn thông báo theo `Outcome`.
- Dùng `Uploader` giả (như test của Windows `GatedUploader`) để kiểm: thành công thì link vào clipboard giả; thất bại thì lưu dự phòng; thoát giữa chừng thì `save_pending` ghi tệp đúng một lần (thư mục tạm riêng, dọn sau).
- Không gọi mạng thật trong test thường.

## QA tay (chủ dự án)

1. Có R2 đã nhập ở Tùy chọn (Task 107): chụp vùng, bấm Tải lên: thông báo thành công, link nằm trong clipboard; dán vào trình duyệt mở được ảnh; hạn link đúng theo Tùy chọn.
2. Tắt mạng (Wi-Fi off) rồi Tải lên: thông báo lỗi dễ hiểu và ảnh được lưu vào thư mục lưu.
3. Bấm Tải lên rồi thoát ngay từ menu (khi có thể): ảnh vẫn được lưu, không mất.
4. Chưa cấu hình R2: bấm Tải lên mở Tùy chọn tab R2, không crash.
5. Phím tải lên toàn màn hình (nếu bật) hoạt động như trên.

## Review Focus

1. **Không mất ảnh**: mọi đường thất bại và đường thoát đều để lại ảnh trong thư mục lưu; thứ tự khóa `PendingUploads` đúng; không mạng lúc thoát; `EXIT_BUDGET`.
2. Khóa R2 không lộ; không `unwrap` trên đường chạy thật; luồng nền không gọi AppKit.
3. Bốn nút của thanh đúng thứ tự, hit-test đúng ở Retina và sát mép; không làm hỏng ba nút cũ.
4. Dùng đúng `run_action`/`Ctx`; không tự dựng URL.
5. Job `windows` vẫn xanh.

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

Task 106 và 107.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
