# Task 114: Sửa và bổ sung bảng kiểu (phản hồi QA của chủ dự án, 2026-11-21)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/114-macos-style-fixes` (từ `feat/macos-port`) · **Rà soát:** hoãn · **Cổng:** `scripts/check.sh`, `scripts/check-windows-cross.sh`, CI

Nền: Task 112–113 (`annotate_style.rs`, `annotate_ui.rs`, `annotate_draw.rs`, `overlay_view.rs`, core `annotate.rs`/`edit.rs`). Không đổi hành vi Windows; `Style` mới chỉ cộng thêm.

## Yêu cầu

1. **Trạng thái Nền/Không nền loại trừ nhau (Hình và Chữ).** Khi chọn "Không nền" thì chip "Nền" KHÔNG được tô nổi (focus); khi chọn lại một màu thì "Nền" nổi và "Không nền" tắt. Đúng một trong hai bật. Quy tắc chọn trạng thái nằm trong hàm thuần có test.
2. **"Không viền" (No Border):** thêm ô tích (checkbox) "Không viền" ở phần Viền của Hình và Chữ. Bật thì không vẽ viền (và không tính vào hộp bao); tắt thì viền trở lại với độ dày và màu trước đó (không mất giá trị cũ). Chuỗi qua `darkshot_core::i18n`.
3. **Bo góc viền (Border Radius) theo px** cho Hình chữ nhật và Chữ (Elip không áp dụng, ẩn hàng này với Elip). Hàng bước −/+ như các hàng khác, 0..=hợp lý (kẹp theo nửa cạnh ngắn của hộp khi vẽ), áp cho cả nền và viền, đốt đúng vào ảnh khi Lưu/Chép/Tải lên. Đơn vị người dùng thấy là px ảnh.
4. **Highlight tự do thiếu độ trong suốt:** thêm hàng Độ trong suốt (10%..100%, bước 10%) cho chế độ tự do, áp cho nét đang chọn (một bước hoàn tác) và làm mặc định, nhớ qua `Prefs`. Hiện nét đã vẽ giữ 0,4 nếu chưa chỉnh (không đổi ảnh cũ). Nếu `Shape::Marker` không mang độ trong suốt, cộng thêm cách lưu mà không đổi nghĩa biến thể cũ ở Windows.
5. **Tooltip (nhãn công cụ khi trỏ) bị che/cắt ở Mũi tên và Đường:** tooltip phải hiện đủ, không bị cắt bởi mép bảng, thanh công cụ hay màn hình, không bị che bởi bảng khác. Tìm nguyên nhân gốc (cắt vùng vẽ, thứ tự vẽ, hay vị trí) và sửa; thêm test bố cục thuần nếu có thể (tooltip nằm trọn trong màn hình, tính cho mọi công cụ).
6. **Chữ: nhấp đúp để sửa.** Khi đang ở công cụ Chọn (hoặc bất kỳ công cụ nào) nhấp đúp vào chữ đã vẽ phải vào chế độ sửa nội dung (đặt caret, giữ kiểu và lớp, một bước hoàn tác khi chốt, Esc hủy thay đổi). Không làm hỏng chọn/di chuyển bằng nhấp đơn và kéo.
7. **Bảng chọn màu/thuộc tính che vùng đang chụp:** bảng không được che vùng chọn (vùng làm việc). Ưu tiên đặt ngoài vùng chọn trên màn hình (cạnh trái/phải/trên/dưới, chọn bên còn chỗ). Nếu không bên nào đủ chỗ: thu gọn (một hàng chip, lưới màu thu nhỏ) hoặc có nút thu/mở, và cho phép kéo bảng; tuyệt đối không phủ lên vùng chọn khi vẫn còn cách tránh. Test thuần: với vùng chọn nhiều cỡ/vị trí (sát mép, toàn màn hình), bảng không giao với vùng chọn khi còn chỗ, luôn nằm trong màn hình, và không chồng thanh công cụ.

## Kiểm thử

TDD, test thuần trước. Pixel test cho bo góc (góc bị bo trống/trộn đúng), Không viền, Highlight tự do có độ trong suốt khác nhau. Mở rộng ảnh mẫu `annotate_sample`. Đường thoát Esc/Hủy/Cmd+Tab không hỏng.

## Global Constraints, Báo cáo

Như Task 112 (không `unwrap` trên đường chạy thật, i18n, không thêm phụ thuộc, không sửa `memory-bank/`, không đẩy, commit có dòng `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`). Cổng: `cargo fmt --all`, `./scripts/check.sh` ALL CHECKS PASSED, `./scripts/check-windows-cross.sh`. Báo cáo tiếng Việt: từng mục 1–7 làm thế nào, tệp, hạn chế, token.
