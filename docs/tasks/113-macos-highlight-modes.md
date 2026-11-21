# Task 113: Highlight hai chế độ (tự do chỉnh cỡ, và dạng hình chữ nhật)

**Agent:** `builder` · **Model:** sonnet · **Nhánh:** `task/113-macos-highlight-modes` (từ `feat/macos-port` sau khi gộp 112) · **Rà soát:** hoãn · **Cổng:** như Task 112

Phụ thuộc Task 112 (kiểu `Style`, bảng thuộc tính, hộp bao/đốt ảnh).

## Mục tiêu

Công cụ Highlight (Marker) có **hai chế độ**, chuyển được ngay trên bảng thuộc tính:
1. **Tự do (freehand):** như hiện nay, thêm chỉnh **độ to nhỏ** (độ dày nét highlight) bằng bảng thuộc tính; áp dụng cho nét đang chọn và nét vẽ tiếp.
2. **Hình (shape):** kéo chuột tạo **hình chữ nhật** tô màu với độ trong suốt như highlight hiện tại, **không viền**; chỉnh được màu và độ trong suốt. Mục đích: highlight không bị lệch hàng khi tô nhiều dòng thẳng.

Chế độ được nhớ làm mặc định cho lần vẽ tiếp. Highlight hình chữ nhật chọn, di chuyển, đổi cỡ (8 tay nắm), hoàn tác/làm lại, z-order và đốt vào ảnh như các hình khác. Giữ nguyên hành vi Marker cũ ở Windows và biến thể `Shape::Marker` hiện có (cộng thêm biến thể mới nếu cần, Windows chỉ thêm nhánh tối thiểu).

## Kiểm thử (TDD)

Pure: chuyển chế độ không đổi nét đã vẽ; kéo ở chế độ hình ra đúng hình chữ nhật chuẩn hóa (kéo ngược chiều, kích thước 0 bị bỏ); đổi độ dày/độ trong suốt khi đang chọn = một bước hoàn tác; hộp bao. Pixel: highlight hình trộn đúng độ trong suốt, không có viền, đốt đúng vị trí trên ảnh Retina; thêm vào ảnh mẫu.

## Global Constraints, Báo cáo

Như Task 112.
