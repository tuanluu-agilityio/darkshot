# Đợt M: bản macOS — kế hoạch

Thiết kế: `docs/superpowers/specs/2026-11-21-macos-port-design.md`. Nhánh tích hợp `feat/macos-port`; mỗi task một nhánh `task/1NN-…` tách từ và gộp lại vào nhánh tích hợp (quyết định 127). Global Constraints: mục cùng tên của `docs/superpowers/plans/2026-11-18-step-capture.md`, cộng các ràng buộc ở spec mục 5 và 6. Model chọn theo độ rủi ro: bảo mật hoặc toàn vẹn dữ liệu không dùng `haiku`; rà soát bởi agent khác, model mạnh bằng hoặc hơn.

| Bước | Task | Nội dung | Agent / model | Rủi ro |
|---|---|---|---|---|
| M0 | (phiên chính) | Cài Rust, test core trên Mac | xong | thấp |
| M1 | (phiên chính) | Thiết kế, kế hoạch, quyết định 127–129 | xong | thấp |
| M2a | 100 | Workspace dùng `cfg` theo hệ điều hành; crate `darkshot-macos` khung; `scripts/check.sh`; `check.yml` | phiên chính (làm trực tiếp) | trung bình |
| M2b | 101 | Chuyển logic thuần từ `main.rs`, `capture.rs`, `paths.rs` sang core (giữ test) | builder / sonnet; rà soát specialist | trung bình (chạm Windows) |
| M2c | 102 | Chuyển logic thuần từ `overlay.rs`, `history_layout.rs`, `steps_*` sang core | builder / sonnet | trung bình |
| M3a | 103 | `coords.rs`, `Dirs`, đường dẫn dữ liệu, `SecretStore` bằng Keychain, `Clipboard`, `Notifier` | specialist / opus (bảo mật khóa) | cao |
| M3b | 104 | Thanh menu, ScreenCaptureKit, kiểm quyền Screen Recording | specialist / opus | cao |
| M3c | 105 | Lớp phủ chọn vùng một màn hình Retina, lưu PNG, chép clipboard | specialist / opus | cao |
| M4a | 106 | Phím tắt toàn cục, đa màn hình, tải lên R2, link có hạn | builder / sonnet | trung bình |
| M4b | 107 | Cửa sổ Tùy chọn (3 tab), i18n, giao diện tối | builder / sonnet | trung bình |
| M4c | 108 | Đóng gói `.app`, ký ad-hoc, `scripts/demo-macos.sh`, README, website, memory bank | scaffolder / sonnet | thấp |
| M5–M7 | 109+ | Đánh dấu, Ghim, Lịch sử, bắt cửa sổ, OCR, Chụp từng bước, tự khởi động, kiểm tra bản mới: tách từng task sau khi M-A được duyệt | — | — |

Cổng mỗi task: `scripts/check.sh` (Mac) phải in `ALL CHECKS PASSED`; task chạm lớp Windows (100, 101, 102) phải qua job `windows` của `.github/workflows/check.yml` (chạy `scripts/check.ps1` trên runner `windows-latest`) trước khi gộp vào `main`, vì chủ dự án không có máy Windows (2026-11-21). Không gộp `main` khi job đó chưa xanh; cần đẩy nhánh lên `origin` để chạy.
