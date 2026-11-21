# Task 110: vẽ và đốt hình đánh dấu bằng Core Graphics (đợt M, giai đoạn M5 bước 1/2)

**Agent:** `builder` · **Model:** sonnet (chủ dự án chọn; haiku không phù hợp: mã vẽ Core Graphics/CoreText và giao diện Objective-C) · **Nhánh:** `task/110-macos-annotate-draw` (từ `feat/macos-port`) · **Rà soát:** bỏ qua theo quyết định của chủ dự án (2026-11-21); builder TỰ rà soát kỹ theo Review Focus · **Cổng:** job `macos` và `windows` của `.github/workflows/check.yml`

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md`. Quyết định 127–142 (đặc biệt 131, 133, 135, 142). Tiền lệ đã chạy thật: Task 103–106; 107–109 đã gộp nhưng chưa rà soát.

## Mục tiêu

Phần "vẽ" của công cụ đánh dấu trên Mac, tương đương `crates/darkshot-windows/src/annotate_draw.rs` (GDI+) nhưng bằng Core Graphics và CoreText: vẽ mọi loại `Shape` của core lên một ngữ cảnh đồ họa, đo hộp bao của chữ, tính khung con trỏ nhập chữ, và **đốt (burn) các hình vào ảnh cuối** để Lưu/Chép/Tải lên ra ảnh có mũi tên. Task 111 làm giao diện công cụ; gói này KHÔNG có công cụ nào để người dùng chọn.

## Cần đọc

`crates/darkshot-core/src/{annotate,edit,overlay_stroke,image,color,geometry}.rs` (`Shape` 9 biến thể: Pen, Line, Arrow, Rect, Marker, Blur, Text, Ellipse, Step; `Tool`, `Pt`, `SizeLevel`, `PALETTE`, các hằng `STROKE_DIP`, `MARKER_DIP`, `MARKER_ALPHA`, `TEXT_DIP`, `TEXT_OUTLINE_DIP`, `ARROW_*`, `STEP_RING`, `arrow_geometry`, `arrow_min_head`, `outline_color`, `blur_pixel_rect`, `Shape::bounds()` trả `None` cho Text, `TextEdit`), `crates/darkshot-windows/src/annotate_draw.rs` (TOÀN BỘ: `draw_shapes`, `draw_one`, `draw_arrow`, `draw_marker`, `draw_blur`, `draw_text`, `draw_step`/`draw_step_number`, `shape_bounds`, `text_bounds`, `caret_rect`, `burn`; đây là đặc tả hành vi thị giác cần khớp), `crates/darkshot-macos/src/{overlay,overlay_view,overlay_input,capture,app}.rs`, `crates/darkshot-macos/src/capture.rs` (`to_bgra`, bố cục BGRA premultiplied-first little-endian).

## Files

Tạo `crates/darkshot-macos/src/annotate_draw.rs`; sửa `main.rs` (khai báo module dưới `cfg`), `overlay.rs`/`overlay_view.rs` (vẽ hình lên lớp phủ trong vùng chọn), `app.rs` (đốt hình khi Lưu/Chép/Tải lên), `Cargo.toml` (feature `objc2-core-graphics`/`objc2-core-text`/`objc2-core-foundation` cần thiết, ghim `=`; nếu CoreText cần crate `objc2-core-text` hãy kiểm tra có bản 0.3.x; nếu không thì dùng FFI `extern "C"` tối thiểu có `// SAFETY:` và nêu lý do).

## Interfaces (tên và ngữ nghĩa theo Windows; tọa độ hình là TỌA ĐỘ ẢNH CHỤP, pixel, gốc trên trái)

```rust
/// Vẽ mọi hình lên `ctx`. `(ox, oy)` là gốc của vùng cắt trong tọa độ ảnh chụp (trừ đi khi vẽ), `clip` là vùng được phép chạm (tọa độ ảnh chụp). Trả `false` nếu có hình không vẽ được.
pub fn draw_shapes(ctx: &CGContext, shapes: &[Shape], ox: i32, oy: i32, clip: Rect) -> bool;
/// Hộp bao (tọa độ ảnh chụp) của một hình; Text đo bằng CoreText cùng phông với lúc vẽ; ước lượng rộng tay như Windows khi không đo được.
pub fn shape_bounds(s: &Shape) -> Rect;
/// Khung con trỏ nhập chữ của `TextEdit`.
pub fn caret_rect(edit: &TextEdit) -> Rect;
/// In `shapes` vào ảnh đã cắt từ vùng `crop` (tọa độ ảnh chụp). Ảnh giữ alpha 255. Rỗng thì không làm gì.
pub fn burn(img: &mut Image, shapes: &[Shape], crop: Rect) -> Result<(), String>;
```
`burn` dựng `CGBitmapContext` trên chính bộ đệm BGRA của `Image` (premultiplied-first, little-endian, sRGB), lật trục để gốc ở trên trái, bật khử răng cưa; không chép toàn ảnh nếu tránh được.

## Phạm vi

1. **Từng loại hình** giống Windows về hình dạng và quy ước (xem `draw_one` và các `draw_*`): Pen (nét tròn đầu tròn góc, mượt), Line, Arrow (thân + đầu theo `arrow_geometry` của core), Rect (khung rỗng), Ellipse (khung rỗng), Marker (nét dày bán trong suốt `MARKER_ALPHA`, đầu vuông), Blur (che bằng màu `color` trên vùng `blur_pixel_rect`, như Windows: kiểm cách `draw_blur` làm rồi làm tương đương), Text (phông hệ thống, kích thước `size` pixel, viền ngoài `outline_color` dày `TEXT_OUTLINE_DIP` theo tỉ lệ, nhiều dòng theo `\n`), Step (hình tròn có vòng `STEP_RING`, số `n` ở giữa, màu chữ tương phản).
2. **Đo chữ**: `shape_bounds` cho Text và `caret_rect` bằng CoreText (`CTLine`/`CTFramesetter`), cùng phông với `draw_shapes`; có đường dự phòng ước lượng như Windows.
3. **Nối vào lớp phủ (không có công cụ)**: `Picked`/kết quả của lớp phủ mang theo `shapes: Vec<Shape>` (rỗng ở gói này); `overlay_view` vẽ `shapes` lên trên ảnh trong vùng chọn (cắt theo vùng chọn); `app.rs` khi Lưu/Chép/Tải lên: cắt ảnh theo vùng rồi `burn(&mut cropped, &shapes, crop_rect)` TRƯỚC `run_action` (trên luồng nền, như hiện tại).
4. **Không công cụ UI** trong gói này. Mọi test hình vẽ chạy ngoài màn hình.

## Kiểm thử (viết trước; CHẠY ĐƯỢC không cần GUI vì `CGBitmapContext` và CoreText hoạt động ngoài màn hình, nên đây là test thường, KHÔNG `#[ignore]`)

Trên ảnh tổng hợp (nền trắng hoặc xám phẳng) kiểm pixel: Rect khung rỗng có pixel màu ở cạnh và nền ở tâm; Line/Arrow có pixel màu dọc đường và đầu mũi tên rộng hơn thân; Ellipse; Pen qua nhiều điểm; Marker bán trong suốt (pha màu với nền, không phủ kín); Blur phủ đúng `blur_pixel_rect` bằng màu cho trước; Step có pixel màu trong vòng và chữ số khác nền; Text: hộp bao rộng hơn khi chữ dài hơn/cỡ lớn hơn, nhiều dòng cao hơn, viền ngoài khác màu chữ; `caret_rect` đúng vị trí theo `caret()`; `burn` với `crop` lệch (`ox, oy`) đặt hình đúng chỗ, cắt đúng ở mép ảnh, ảnh vẫn alpha 255, danh sách rỗng không đổi pixel nào, không panic với hình ngoài ảnh hoặc điểm NaN/vô cực/rất lớn. Mọi hình rỗng điểm không vẽ gì. Tạo thêm một test `#[ignore]` (chạy tay, an toàn, chỉ ghi tệp) ghi một PNG chứa MỖI loại hình vào thư mục tạm `std::env::temp_dir()/darkshot-annotate-sample.png` và in đường dẫn, để chủ dự án mở xem bằng mắt.

## QA tay (chủ dự án)

1. Chạy test `#[ignore]` ghi PNG mẫu (lệnh: `cargo test -p darkshot-macos --bin darkshot-macos -- --ignored annotate_sample --test-threads=1`) rồi mở ảnh: đủ chín loại hình, trông gần giống ảnh mẫu của bản Windows (mũi tên có đầu, viền chữ, số trong vòng tròn, marker trong suốt).
2. (Chưa có công cụ để vẽ trong ứng dụng: phần đó là Task 111.)

## Review Focus (builder TỰ kiểm và nêu kết quả trong báo cáo)

1. Mỗi loại hình khớp Windows (đọc `annotate_draw.rs`) về hình dạng, độ dày theo hằng của core, màu, bán trong suốt; không lệch nửa pixel làm mờ khung; hệ tọa độ (gốc trên trái, lật CTM đúng một lần), `ox/oy`, cắt vùng.
2. `burn`: bộ đệm BGRA đúng bố cục, không đọc/ghi ngoài bộ đệm, alpha 255 sau khi vẽ, không giữ con trỏ đến bộ đệm sau khi trả về, vòng đời `CGContext` đúng.
3. CoreText: vòng đời đối tượng, không rò, không crash với chuỗi rỗng, rất dài, ký tự Unicode (tiếng Việt có dấu, emoji), `\n`.
4. Không `unwrap/expect/panic` trên đường chạy thật; mọi `unsafe` có `// SAFETY:`.
5. Đường Lưu/Chép/Tải lên với `shapes` rỗng giữ NGUYÊN hành vi hiện tại (không hồi quy), không chép ảnh thừa.
6. Job `windows` vẫn xanh (mọi mã dưới `cfg(target_os = "macos")`).

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

Task 105, 108 (đã gộp).

## Hoàn tất

`cargo fmt --all`; `./scripts/check.sh` → `ALL CHECKS PASSED`. Báo cáo ngắn: mục đã làm và nơi đặt; chuỗi i18n đã thêm; API Objective-C/C dùng và phiên bản đã ghim; mọi chỗ lệch gói và lý do; điều không kiểm được tự động (để QA tay); số token. Chưa tự đẩy nhánh.
