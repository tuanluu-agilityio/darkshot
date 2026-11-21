# Task 111: giao diện công cụ đánh dấu trên lớp phủ macOS (đợt M, giai đoạn M5 bước 2/2)

**Agent:** `builder` · **Model:** sonnet (chủ dự án chọn; haiku không phù hợp: mã vẽ Core Graphics/CoreText và giao diện Objective-C) · **Nhánh:** `task/111-macos-annotate-ui` (từ `feat/macos-port`) · **Rà soát:** bỏ qua theo quyết định của chủ dự án (2026-11-21); builder TỰ rà soát kỹ theo Review Focus · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml`

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md`. Quyết định 127–142 (đặc biệt 131, 133, 135, 142). Tiền lệ đã chạy thật: Task 103–106; 107–109 đã gộp nhưng chưa rà soát.

## Mục tiêu

Người dùng chọn công cụ và vẽ trên vùng chọn: bút, đường, mũi tên, hình chữ nhật, elip, marker, làm mờ, chữ, đánh số; chọn, di chuyển, đổi cỡ và sửa hình đã vẽ; chọn màu và cỡ; Hoàn tác/Làm lại nhiều bước. Hình được đốt vào ảnh cuối bằng `burn` của Task 110. Hành vi tham chiếu: lớp phủ Windows (`crates/darkshot-windows/src/overlay.rs`); logic dùng chung đã ở core.

## Cần đọc

`crates/darkshot-core/src/{annotate,edit,overlay_stroke,overlay_layout,color_picker,color,geometry,i18n}.rs`. Đặc biệt: `Annotations` (`add`, `replace`, `remove`, `undo`, `redo`, `can_undo/can_redo`, `next_step_number`), `Tool` (+`from_key`), `SizeLevel`, `PALETTE`/`PALETTE_COLS`/`DEFAULT_COLOR`, `press_mode`, `PressMode`, `TextEdit`, `edit::{Gesture, hit_shape, topmost_hit, handles, grip_at, frame, transform, restyle}`, `overlay_stroke::{Stroke, begin_stroke, extend_stroke, end_stroke, segment_dirty}`, `overlay_layout::{SideItem, SIDE_ITEMS, side_layout_for, side_item_rect, side_item_at, size_cell_*, shape_cell_*, palette_rect_for, flyout_rect, BarItem::{Undo,Redo}, bar_cells}`, `color_picker::*`. `crates/darkshot-windows/src/overlay.rs` (CHỈ phần công cụ đánh dấu: máy trạng thái chuột/phím khi có công cụ, bảng phím công cụ/cỡ/hoàn tác, chữ, bảng màu/cỡ/hình, thanh dọc; bỏ qua cửa sổ/chọn vùng đã làm), `crates/darkshot-macos/src/{overlay,overlay_view,overlay_input,annotate_draw,app}.rs` (Task 105, 110).

## Files

Sửa `overlay_input.rs` (mô hình thuần: trạng thái công cụ, cử chỉ, văn bản, bảng), `overlay_view.rs` (vẽ thanh dọc, bảng, hình đang vẽ; sự kiện chuột/phím), `overlay.rs`, `app.rs` (chuyển `shapes` sang `burn` — đã nối ở Task 110); tạo `crates/darkshot-macos/src/annotate_ui.rs` (mô hình thuần của công cụ) nếu hợp lý; sửa `i18n.rs` chỉ để thêm chuỗi Mac (gợi ý phím kiểu ⌘, đủ hai bản).

## Phạm vi (làm theo thứ tự; mỗi bước commit riêng)

1. **Mô hình thuần có test (TDD)**: trạng thái công cụ (công cụ hiện tại, hình nhớ, `SizeLevel`, màu), `Annotations`, cử chỉ chuột theo `PressMode` (vẽ mới / chọn / di chuyển / đổi cỡ tay nắm), phím (chọn công cụ bằng `Tool::from_key`, tăng/giảm cỡ, ⌘Z hoàn tác, ⇧⌘Z làm lại, Xóa/Delete xóa hình chọn, Esc theo Windows: bỏ chọn/kết thúc chữ/hủy), `TextEdit` (chèn, xóa, mũi tên, xuống dòng, dán qua `sanitize_paste`), con trỏ theo ngữ cảnh. Tách khỏi AppKit để kiểm bằng test thường.
2. **Thanh công cụ dọc** (`side_layout_for`/`side_item_rect`/`side_item_at` của core) và nút Hoàn tác/Làm lại trên thanh ngang (`BarItem::Undo/Redo`): vẽ biểu tượng (Core Graphics, đơn giản), trạng thái chọn/mờ, gợi ý (`SideItem::tip` + chuỗi Mac nếu phím tắt khác), hit-test ở Retina và sát mép.
3. **Vẽ bằng công cụ**: bút/marker (`overlay_stroke`), đường/mũi tên/khung/elip (kéo thả, Shift ràng buộc 45°/vuông theo `constrain_*` của core), làm mờ, đánh số (`next_step_number`), chữ (nhấn đặt, gõ, con trỏ nhấp nháy theo `caret_rect`, Enter xuống dòng, bấm ra ngoài hoặc Esc kết thúc); hình đang vẽ hiện ngay (vẽ lại cục bộ bằng `segment_dirty`/vùng bẩn, không vẽ lại toàn màn hình mỗi sự kiện).
4. **Chọn và sửa hình**: công cụ Chọn (`Tool::Select`), `topmost_hit`, tay nắm (`handles`/`grip_at`), `Gesture` + `transform`, `restyle` khi đổi màu/cỡ lúc đang chọn hình.
5. **Bảng Cỡ, bảng Hình, bảng màu** (`size_cell_*`, `shape_cell_*`, `palette_rect_for`): chọn màu từ `PALETTE` (48 màu) và cỡ. **Bộ chọn màu tùy chỉnh (phổ + độ sáng + ô nhập số HLS/RGB, `color_picker::*`) là TÙY CHỌN: làm cuối cùng nếu còn dư sức, KHÔNG bắt buộc**; nếu bỏ thì ghi rõ trong báo cáo (không đưa nút chưa làm ra giao diện).
6. **Kết quả**: nút Lưu/Chép/Tải lên chuyển `shapes` của `Annotations` cho `burn` (Task 110); bố cục thanh ngang bây giờ có Hoàn tác/Làm lại + 4 nút hành động; `Esc`/Hủy vẫn thoát lớp phủ mọi lúc (không bao giờ kẹt: giữ lưới an toàn 120 s, tương tác công cụ cũng tính là tương tác).

## Chuỗi i18n

Dùng `Msg` sẵn có của core cho tên công cụ/cỡ/màu; thêm chuỗi Mac (đủ hai bản) chỉ khi phím tắt hiển thị khác Windows.

## Kiểm thử (viết trước)

- Thuần: máy trạng thái cử chỉ và phím (mỗi công cụ tạo đúng `Shape` với `add`; hoàn tác/làm lại nhiều bước; chọn/di chuyển/đổi cỡ qua `Gesture`; Shift ràng buộc; chữ đa dòng; bỏ chọn khi đổi công cụ), bố cục thanh dọc và bảng ở `dpi` 96/144/192/288 và bốn góc/mép màn hình (không tràn, không chồng thanh ngang, không che vùng chọn khi tránh được), hit-test từng ô, ánh xạ phím → hành động, chọn màu/cỡ.
- Không test chạm GUI/hiện cửa sổ trong test thường; test GUI chỉ `#[ignore]` (cờ ẩn `--overlay-selftest` đã có; có thể mở rộng để mở lớp phủ với một vùng chọn sẵn).

## QA tay (chủ dự án)

1. Chụp, kéo chọn vùng: thanh công cụ dọc xuất hiện; chọn từng công cụ (bút, đường, mũi tên, khung, elip, marker, làm mờ, chữ, đánh số) và vẽ trên vùng chọn; hình hiện ngay, không giật.
2. Đổi màu và cỡ; Shift khi vẽ đường/khung cho góc 45° và hình vuông.
3. Chữ: nhấn đặt, gõ tiếng Việt có dấu và nhiều dòng, Enter xuống dòng, nhấn ngoài để kết thúc.
4. Công cụ Chọn: bấm hình để chọn, kéo để di chuyển, kéo tay nắm đổi cỡ; đổi màu/cỡ khi đang chọn hình thì hình đổi theo.
5. ⌘Z/⇧⌘Z hoặc nút Hoàn tác/Làm lại nhiều bước; Xóa bỏ hình đang chọn.
6. Lưu/Chép/Tải lên: ảnh ra CÓ các hình đã vẽ, đúng vị trí và kích thước (Retina gấp đôi), không lệch.
7. `Esc` luôn thoát; bấm Hủy thoát; Cmd+Tab thoát; không kẹt lớp phủ ở bất kỳ công cụ nào.

## Review Focus (builder TỰ kiểm và nêu kết quả trong báo cáo)

1. **Đường thoát**: không trạng thái công cụ nào (đang gõ chữ, đang kéo, bảng mở) làm `Esc`/Hủy/lưới 120 s mất tác dụng.
2. Trạng thái kéo thả của vùng chọn (`Selection`) và của hình (`Annotations`/`Gesture`) không lẫn nhau: bấm trên hình khác với bấm tay nắm vùng chọn (theo `press_mode`); vẽ ngoài vùng chọn không tạo hình.
3. Tọa độ: hình lưu theo tọa độ ảnh chụp (px), đúng ở Retina và màn hình phụ gốc âm; `burn` ra đúng chỗ.
4. Vẽ lại cục bộ đúng (không sót vệt, không vẽ lại toàn màn hình mỗi di chuột); bộ nhớ không phình khi vẽ lâu.
5. Chữ: `TextEdit` của core là nguồn duy nhất; nhập IME/tiếng Việt có dấu không mất ký tự (kiểm cách nhận `insertText:` hay `keyDown:`; nêu rủi ro còn lại nếu chưa hỗ trợ IME đầy đủ).
6. Vòng đời đối tượng/`define_class!` không rò; không gọi AppKit từ luồng nền; job `windows` vẫn xanh; không hồi quy Lưu/Chép/Tải lên/Hủy/⌘S/⌘C/⌘D.

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

Task 110.

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
