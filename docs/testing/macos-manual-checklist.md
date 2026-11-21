# Danh sách kiểm tay bản macOS

Đánh dấu `[x]` khi đã thử trên máy thật. Mục chưa thử để `[ ]`. Không đưa khóa R2, ảnh chụp màn hình thật hay đường dẫn cá nhân vào tệp này (repo công khai).

Trạng thái: Task 103 đến 106 ĐÃ đạt (chủ dự án, 2026-11-21). Task 107, 108 và 109A CHƯA thử; phải thử trước khi gộp `feat/macos-port` vào `main`.

## Cách chạy

- Chạy thử cách ly: `scripts/demo-macos.sh --release`. Script dựng gói, đặt `DARKSHOT_PROFILE_DIR` ra thư mục tạm, mở bản dựng và dọn mục Keychain `darkshot-sandbox-…` khi kết thúc. Không bao giờ dùng service `darkshot` thật.
- Hồ sơ cố định để thử nhiều lần: `mkdir -p /tmp/ds-qa` rồi `open -n --env "DARKSHOT_PROFILE_DIR=/tmp/ds-qa" target/macos/darkshot.app`. Cấu hình nằm ở `/tmp/ds-qa/Roaming/darkshot/config.json`, ảnh ở `/tmp/ds-qa/Pictures/darkshot`.
- Ký ổn định (để khỏi bị hỏi lại quyền sau mỗi lần build): `export DARKSHOT_SIGN_IDENTITY="<tên chứng chỉ>"`, xem `macos-local-signing.md`.
- Bật phím tắt chụp toàn màn hình khi cần thử: cách dễ nhất là mở Tùy chọn (tab Phím tắt). Hoặc tạo `config.json` của hồ sơ cách ly khi ứng dụng đã tắt, với ĐÚNG nội dung dưới đây (đã kiểm bằng `config::load` ở Task 106; mã phím theo VK của Windows: `51` = `3`, `52` = `4`; ghi đủ mọi trường, vì trường thiếu âm thầm rơi về mặc định PrintScreen). Chụp vùng `Control+Shift+4`, lưu toàn màn hình `Control+Shift+3`:

```json
{"language":"vi","hotkey":{"ctrl":true,"alt":false,"shift":true,"win":false,"vk":52},"hotkey_enabled":true,"hotkey_save_full":{"enabled":true,"ctrl":true,"alt":false,"shift":true,"win":false,"vk":51}}
```

  Để bật thêm tải lên toàn màn hình (`Control+Option+3`), thêm vào trước dấu `}` cuối: `,"hotkey_upload_full":{"enabled":true,"ctrl":true,"alt":true,"shift":false,"win":false,"vk":51}`. Thêm `"language":"en"` thay `"vi"` nếu muốn tiếng Anh.

- Dọn Keychain sau khi thử hồ sơ cố định: `security delete-generic-password -s <service> -a r2-secret-access-key`, với `<service>` lấy bằng `DARKSHOT_PROFILE_DIR=/tmp/ds-qa target/macos/darkshot.app/Contents/MacOS/darkshot --keychain-service` (chỉ in tên, không chạm Keychain). Rồi `rm -r /tmp/ds-qa`.

## Task 103: nền móng (ĐÃ đạt)

- [x] `cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1`: mọi test qua (có thể hiện hộp thoại Keychain, bấm Cho phép).
- [x] Keychain Access không còn mục `darkshot-test-…` sót lại.

## Task 104: vỏ ứng dụng (ĐÃ đạt)

- [x] Biểu tượng trên thanh menu, không có biểu tượng Dock; menu chỉ có mục đã làm, chữ đúng ngôn ngữ hệ thống.
- [x] Lần chụp đầu hệ thống hỏi quyền Ghi màn hình; thiếu quyền thì có thông báo hướng dẫn, không có ảnh rỗng.
- [x] Ảnh lưu có kích thước pixel đúng màn hình (Retina gấp đôi điểm).
- [x] Build lại cùng chứng chỉ không hỏi lại quyền; build ad-hoc thì script cảnh báo.
- [x] Mở bản thứ hai cho cùng hồ sơ: thoát ngay, không có biểu tượng thứ hai. Thoát từ menu sạch.
- [x] Keychain: từ chối truy cập thì hiện luồng "mất khóa", không crash; khóa thật (service `darkshot`) không đổi khi chạy demo cách ly.

## Task 105: lớp phủ chọn vùng (ĐÃ đạt)

- [x] Kéo chọn vùng, nhãn `W × H` đúng; thanh nút hiện cạnh vùng, nằm trong màn hình kể cả sát mép và tai thỏ.
- [x] Di chuyển và đổi cỡ bằng tám tay nắm.
- [x] Lưu ra PNG đúng pixel (200×100 điểm Retina ra 400×200 px), không lệch, không lật. Chép dán đúng ảnh.
- [x] `Esc` và Hủy đóng sạch, chụp lại được ngay (kể cả `Esc` khi chưa bấm chuột).
- [x] Nhiều màn hình: chọn ở đâu lưu đúng màn hình đó; không kéo vùng sang màn hình kia.
- [x] Ứng dụng toàn màn hình (Space riêng): lớp phủ vẫn trên cùng. `Cmd+Tab` khi đang chọn không để lại lớp phủ. Đổi cấu hình màn hình khi đang chọn: hủy sạch.

## Task 106: phím tắt toàn cục (ĐÃ đạt)

- [x] `Control+Shift+4` ở ứng dụng bất kỳ mở lớp phủ.
- [x] Bật `Control+Shift+3` thủ công: ảnh toàn màn hình chứa con trỏ được lưu.
- [x] Phím bị ứng dụng khác chiếm: thông báo nêu đúng phím, không crash.
- [x] Bấm phím khi lớp phủ đang mở hoặc đang chụp: không mở lớp phủ thứ hai.

## Task 107: cửa sổ Tùy chọn (CHƯA)

- [ ] Menu, Tùy chọn: cửa sổ tối, 4 tab, chữ đúng cả Việt và English (đổi ngôn ngữ rồi mở lại).
- [ ] Tab R2: chính BẠN nhập thông tin R2 (không đưa khóa cho ai), bấm Kiểm tra kết nối: thành công hoặc lỗi dễ hiểu, giao diện không treo. Lưu rồi mở lại: ô khóa trống (giữ khóa cũ); khóa nằm trong Keychain.
- [ ] Nhập sai (URL công khai hỏng, trùng phím tắt): hiện lỗi đúng, chuyển đúng tab, không đóng cửa sổ.
- [ ] Đổi phím chụp vùng bằng ô bắt phím rồi OK: phím mới có hiệu lực ngay, phím cũ hết; phím bị chiếm báo rõ.
- [ ] Hủy: không đổi gì, tệp cấu hình không bị ghi.
- [ ] Chưa có R2 mà chạy lần đầu: cửa sổ mở thẳng tab R2.

## Task 108: tải lên R2 (CHƯA)

- [ ] Có R2 đã nhập: chụp vùng, bấm Tải lên (hoặc `Cmd+D`): thông báo thành công, link trong clipboard, dán vào trình duyệt mở được ảnh, hạn link đúng theo Tùy chọn.
- [ ] Tắt Wi-Fi rồi Tải lên: thông báo lỗi dễ hiểu, ảnh được lưu vào thư mục lưu.
- [ ] Bấm Tải lên rồi thoát ngay từ menu: ảnh vẫn được lưu, không mất.
- [ ] Chưa cấu hình R2: Tải lên mở Tùy chọn tab R2, không crash.
- [ ] Phím tải lên toàn màn hình (nếu bật) hoạt động như trên.

## Task 109A: thoát an toàn ngoài menu (CHƯA)

Việc thử dùng ảnh chụp thử và hồ sơ cách ly. Để có lượt tải lên "dở" mà không cần mạng chậm: thử khi tắt mạng hoặc dùng R2 trỏ tới địa chỉ không phản hồi, bấm Tải lên rồi thoát ngay trong vài giây.

- [ ] Menu "Thoát" khi có lượt tải lên dở: ảnh được lưu (đúng một tệp), ứng dụng thoát trong khoảng 4 giây.
- [ ] `killall darkshot` (SIGTERM) khi có lượt dở: ảnh được lưu đúng một tệp, ứng dụng thoát. Không có tệp thứ hai.
- [ ] `Ctrl+C` khi chạy bản dựng từ terminal (SIGINT): như trên.
- [ ] Đăng xuất (Apple, Đăng xuất) khi có lượt dở: sau khi đăng nhập lại, ảnh có trong thư mục lưu.
- [ ] Khởi động lại hoặc tắt máy khi có lượt dở: như trên.
- [ ] Không có lượt dở: thoát bằng cách nào cũng không ghi tệp thừa, không treo.
- [ ] Đang mở lớp phủ chọn vùng rồi `killall darkshot`: lớp phủ biến mất, ứng dụng thoát.

## Task 109C: biểu tượng (CHƯA)

- [ ] Ảnh nguồn chỉ 256 px nên `.icns` thiếu cỡ 512 và 1024 (macOS tự co giãn). Sau `scripts/make-icns.sh` rồi `scripts/bundle-macos.sh`: `target/macos/darkshot.app` có `Contents/Resources/darkshot.icns`; biểu tượng hiện trong Finder và hộp thoại quyền. (Ứng dụng là `LSUIElement`, không có biểu tượng Dock.)
