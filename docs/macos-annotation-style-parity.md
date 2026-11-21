# Kiểu đánh dấu mở rộng: Mac đã có, Windows nên theo

Ngày: 2026-11-21 · Nhánh: `feat/macos-port` · Gói task: 112, 113, 114 · Trạng thái: bản Mac chưa rà soát/QA tay; **bản Windows chưa làm** (đề xuất, chờ chủ dự án quyết định)

Tài liệu này không sửa spec đã duyệt (annotate, annotation-pro, compact-toolbar). Quyết định ghi ở `memory-bank/decisions.md` (143).

## 1. Bản Mac đã có gì (khác Windows hiện tại)

| Nhóm | Hành vi trên Mac | Nằm ở |
|---|---|---|
| Hình chữ nhật, Elip (`Shape::Box`) | Màu nền hoặc không nền, màu viền, độ dày viền, độ trong suốt. "Không nền" và "Nền" loại trừ nhau khi hiển thị trạng thái. Ô tích "Không viền". Bo góc theo px (chỉ Hình chữ nhật). | core `Style`; Mac `annotate_style.rs` |
| Chữ (`Shape::Label`) | Căn trái/giữa/phải, nền hoặc không nền (mặc định không), màu và độ dày viền, bo góc, độ trong suốt. Nhấp đúp để sửa lại chữ. | core `Align`, `align_offsets`; Mac |
| Highlight | Hai chế độ: tự do (chỉnh độ dày và độ trong suốt) và hình chữ nhật không viền (`Shape::Highlight`, chỉnh màu và độ trong suốt) để tô thẳng hàng. | core `HighlightMode`, `highlight_from_stroke` |
| Giao diện | Bảng thuộc tính tự hiện theo công cụ hoặc hình đang chọn; đặt ngoài vùng chọn, không che vùng làm việc; tooltip không bị cắt. | Mac |
| Kết quả | Mọi kiểu được đốt vào ảnh khi Lưu, Chép, Tải lên; hình dùng sau nằm trước; sửa kiểu là một bước hoàn tác và giữ nguyên lớp. | |

## 2. Phần dùng chung đã nằm ở core (Windows không phải viết lại)

Kiểu và quy tắc thuần đã ở `darkshot-core` kèm test: `Style`, `Align`, `align_offsets`, `Shape::{Box, Label, Highlight}`, `HighlightMode`, `highlight_from_stroke`, `highlight_restyle`, chọn/di chuyển/đổi cỡ/hộp bao trong `edit.rs`, chuỗi giao diện trong `i18n.rs` (Vi/En). Hiện Windows chỉ thêm nhánh `match` tối thiểu để biên dịch (`annotate_draw.rs::draw_one`, `overlay.rs` test): **không bao giờ tạo** các biến thể này, nên hành vi Windows không đổi.

## 3. Đề xuất cho Windows (để hai bản nhất quán)

Mức ưu tiên theo giá trị cho người dùng và công sức:

1. **Highlight hai chế độ (ưu tiên cao, công sức vừa).** Giải quyết đúng vấn đề tô lệch hàng. Việc: vẽ `Shape::Highlight` bằng GDI+ (một hình tô, độ trong suốt, không viền; `annotate_draw.rs::draw_one`), công cụ Highlight tạo hình khi ở chế độ hình, chuyển chế độ trong thanh công cụ. Có sẵn hit/tay nắm/đổi cỡ ở core.
2. **Hình: nền, viền, độ trong suốt, Không viền (ưu tiên cao, công sức vừa).** Vẽ `Shape::Box` (nền + viền + độ trong suốt, elip khi `oval`), nhánh mới trong `draw_one` và `shape_bounds`; công cụ Hình/Elip tạo `Box`. Cần một bảng thuộc tính tương đương trong `overlay.rs` (hiện thanh công cụ Windows chỉ có Màu/Cỡ/Hình).
3. **Chữ: căn lề, nền, viền (công sức lớn hơn).** Vẽ `Shape::Label` bằng `draw_text`/`text_bounds` hiện có cộng đo từng dòng để căn; nhấp đúp để sửa lại chữ. Làm sau hai mục trên.
4. **Bo góc và độ trong suốt Highlight tự do (nhỏ, làm cùng mục 2 và 1).** GDI+ có đường dẫn bo góc (`GdipAddPathArc`).
5. **Bảng thuộc tính không che vùng chụp** (nếu thêm bảng ở Windows): dùng cùng quy tắc bố cục thuần; nên chuyển hàm bố cục bảng sang core để hai bản dùng chung.

Hệ quả cần quyết định trước khi làm:
- **Tương thích dữ liệu:** hình đã vẽ chỉ sống trong phiên (không lưu), nên không có vấn đề tệp. Lịch sử và Ghim lưu ảnh đã đốt, không lưu hình.
- **Nhất quán đầu ra:** cùng một thao tác (kiểu mặc định "không nền, viền Vừa, đặc") phải cho ảnh gần giống nhau giữa hai bản; dùng ảnh mẫu của Mac (`annotate_sample`) làm đối chiếu.
- **Kiểm chứng:** không có máy Windows (quyết định 130); mọi thay đổi chạm lớp Windows phải qua job `windows` của CI và cần chủ dự án QA trên máy Windows thật trước khi phát hành. Nên làm trên một nhánh riêng sau khi bản Mac ổn định và hoàn tất rà soát opus.
- **Website và README:** cập nhật khi từng phần lên `main` (quy ước dự án), không trước.

## 4. Khoảng trống còn lại (cả hai bản)

Bộ chọn màu tùy chỉnh (Mac mới có 48 màu); khối chữ chưa kéo được hộp rộng tùy ý; đổi cỡ Chữ bằng tay nắm chưa scale độ dày viền.
