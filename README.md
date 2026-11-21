# darkshot

darkshot là ứng dụng Windows 11 chạy ở khay hệ thống. Bấm `PrintScreen`, kéo chọn vùng màn hình (hoặc bấm một lần để chọn đúng một cửa sổ), đánh dấu nếu cần, rồi chọn một trong bốn việc: tải ảnh lên Cloudflare R2 và chép link, lưu PNG vào máy, chép ảnh vào clipboard, ghim ảnh lên màn hình, hoặc hủy. Vùng đã chọn cũng nhận dạng được chữ (Ctrl+T) và chép chữ ra, hoàn toàn trên máy. Có lịch sử ảnh chụp cục bộ (tùy chọn). Ứng dụng viết bằng Rust thuần, không có máy chủ trung gian.

## Tải về

Bản mới nhất: **0.4.0** ở [trang Releases](https://github.com/tuanluu-agilityio/darkshot/releases/latest) (`darkshot-v0.4.0-windows-x64.zip` gồm `darkshot.exe` và README, hoặc chỉ `darkshot.exe`; kiểm tra bằng `SHA256SUMS.txt`). Không cần cài đặt: chạy thẳng `darkshot.exe`. File chưa ký số nên SmartScreen có thể cảnh báo lần đầu: **More info** → **Run anyway**.

Thay đổi của từng phiên bản: [CHANGELOG.md](CHANGELOG.md).

Chụp lại vùng trước, Nhận dạng chữ cục bộ và Chụp từng bước đã có trong mã nguồn ở nhánh `main` nhưng chưa có trong bản 0.4.0; muốn dùng ngay thì build từ mã nguồn (mục dưới). Xem mục "Chưa phát hành" của CHANGELOG.


## Build

Yêu cầu: Rust (cài qua rustup), Windows 11.

```powershell
cargo build --release
```

File chạy nằm tại `target\release\darkshot.exe`. Kích thước bản build 0.4.0 đo ngày 2026-11-18: **2.041.856 byte** (khoảng 1,95 MiB, bản `--release` trên Windows 11). Bản build từ `main` có thêm Chụp lại vùng trước, Nhận dạng chữ cục bộ và Chụp từng bước (đo ngày 2026-11-18): **2.179.072 byte** (khoảng 2,08 MiB).

Biểu tượng ứng dụng nằm ở `crates\darkshot-windows\res\darkshot.ico`, sinh bằng `python crates\darkshot-windows\res\make_icon.py` (cần Pillow). Script ghi đè `darkshot.ico` cạnh nó.

## macOS (Apple silicon)

Bản macOS nằm trên nhánh `feat/macos-port`, **chưa phát hành** (chưa có bản tải về). Chỉ build từ mã nguồn được.

- Yêu cầu: macOS 14 trở lên, Apple silicon (`aarch64-apple-darwin`), Rust (rustup) và Command Line Tools.
- Hiện có: biểu tượng trên thanh menu (không có biểu tượng Dock), chụp vùng bằng lớp phủ chọn trên mọi màn hình (mỗi màn hình một ảnh, vùng chọn không vắt qua màn hình), phím tắt toàn cục, lưu PNG, chép ảnh vào clipboard, tải lên Cloudflare R2 và chép link có hạn, cửa sổ Tùy chọn (R2, phím tắt, ngôn ngữ Việt/English), khóa R2 lưu trong Keychain. Thoát từ menu, đăng xuất, tắt máy hoặc `SIGTERM` đều lưu ảnh của lượt tải lên còn dở vào thư mục lưu thay vì làm mất.
- Phím mặc định trên Mac: `Control+Shift+4` chụp vùng. Phím chụp toàn màn hình và lặp lại tắt sẵn, bật trong Tùy chọn.
- Build và chạy:

```sh
scripts/bundle-macos.sh --release   # ra target/macos/darkshot.app
```

  Nên ký bằng chứng chỉ tự ký cục bộ để macOS không hỏi lại quyền sau mỗi lần build: xem [docs/testing/macos-local-signing.md](docs/testing/macos-local-signing.md). Chạy thử trong hồ sơ cách ly (không dùng cấu hình, ảnh hay khóa thật): `scripts/demo-macos.sh --release`. Danh sách kiểm tay: [docs/testing/macos-manual-checklist.md](docs/testing/macos-manual-checklist.md).
- Quyền cần cấp: **Ghi màn hình** (Screen Recording; hệ thống hỏi lần chụp đầu, có thể phải mở lại ứng dụng sau khi cấp) và **Thông báo**.
- Chưa có trên Mac: ghim ảnh, lịch sử, nhận dạng chữ, bắt cửa sổ khi chọn vùng, chụp từng bước, công cụ đánh dấu, lặp lại lần chụp trước, tự khởi động cùng máy, kiểm tra cập nhật. Chưa ký bằng chứng chỉ Apple và chưa notarize.

## Trang giới thiệu

`website/` chứa trang giới thiệu darkshot (static site Vite + TypeScript, hai ngôn ngữ EN/VI, tự lấy bản phát hành mới nhất từ GitHub). Tách khỏi ứng dụng, không ảnh hưởng `cargo build`. Cách chạy, build và triển khai: [website/README.md](website/README.md).

## Chuyển từ shotme

darkshot là tên mới của shotme. Nếu máy đang có shotme:

1. Thoát shotme (menu khay, **Thoát**) trước khi chạy `darkshot.exe`. Hai bản cùng chạy sẽ tranh phím tắt; khi đó darkshot báo "Phím tắt chưa hoạt động".
2. Lần chạy đầu, darkshot tự chép `%APPDATA%\shotme\config.json` sang `%APPDATA%\darkshot\config.json`, kể cả khóa R2 đã mã hóa. Không phải nhập lại R2. Nếu `%APPDATA%\darkshot\config.json` đã có thì darkshot giữ nguyên file đó, không ghi đè.
3. Khởi động cùng Windows theo cấu hình của darkshot, và mục khởi động của shotme được gỡ. Khi cấu hình được chép từ shotme ở lần chạy đầu, shotme đang bật thì darkshot cũng bật. Nếu darkshot đã có cấu hình riêng với khởi động tắt thì vẫn tắt. Ngoại lệ: nếu `%APPDATA%\darkshot\config.json` hỏng hoặc không đọc được, darkshot không đụng tới mục khởi động nào (của cả hai bản) để không ai mất khởi động cùng Windows.
4. Thư mục ảnh cũ vẫn được dùng: nếu shotme lưu vào `Pictures\shotme` (mặc định cũ), darkshot tiếp tục lưu vào đó. Cấu hình mới của darkshot dùng `Pictures\darkshot`.
5. `%APPDATA%\shotme` không bị xóa. Sau khi thấy darkshot chạy đúng, có thể xóa thư mục này bằng tay.

Nếu một bước chuyển bị lỗi, darkshot vẫn chạy với các thiết lập đang có và hiện thông báo "Chưa chuyển được hết thiết lập từ phiên bản cũ".

## Cách dùng

1. Chạy `darkshot.exe`. Biểu tượng xuất hiện ở khay hệ thống, chú thích "darkshot".
2. Bấm chuột trái hoặc phải vào biểu tượng để mở menu: **Chụp màn hình**, **Chụp lại vùng trước**, **Lịch sử ảnh chụp**, **Chụp từng bước**, **Tùy chọn...**, **Thông tin...**, **Trợ giúp**, **Thoát**.
3. Lần chạy đầu, khi chưa có cấu hình, cửa sổ **Tùy chọn** tự mở ở tab **Lưu trữ R2**. Điền thông tin R2 (xem mục bên dưới) rồi bấm **OK**.
4. Bấm `PrintScreen` hoặc chọn **Chụp màn hình**. Màn hình tối đi và đứng yên.
5. Kéo chuột để chọn vùng. Vùng chọn sáng, có nhãn kích thước `{rộng} × {cao}` tính bằng pixel. Hoặc rê chuột lên một cửa sổ (hay nền của một màn hình) rồi bấm một lần, hoặc Enter, để chọn đúng khung đó (xem "Gợi ý cửa sổ").
6. Chỉnh vùng chọn nếu cần: kéo 8 tay nắm ở góc và cạnh để đổi kích thước, kéo bên trong để di chuyển (khi không chọn công cụ đánh dấu), kéo ở ngoài khung để chọn lại từ đầu (khi chưa có nét vẽ).
7. Chọn hành động bằng nút trên thanh công cụ hoặc phím tắt.

### Thanh công cụ

Thanh có tám nút chỉ có biểu tượng: Hoàn tác, Làm lại, một vạch ngăn, rồi sáu nút hành động (Tải lên, Lưu, Chép, Ghim, Nhận dạng chữ, Hủy). Khi đang có phiên chụp từng bước, thanh có thêm nút **Thêm bước** ngay trước Hủy. Rê chuột vào nút để xem tên và phím tắt.

| Nút | Phím | Việc làm |
|---|---|---|
| Hoàn tác | `Ctrl+Z` | Bỏ thay đổi đánh dấu gần nhất (xem "Đánh dấu ảnh"). Mờ khi không có gì để bỏ. Lớp phủ vẫn mở. |
| Làm lại | `Ctrl+Y` hoặc `Ctrl+Shift+Z` | Làm lại thay đổi vừa hoàn tác. Mờ khi không có gì để làm lại. Lớp phủ vẫn mở. |
| Tải lên và chép link | `Ctrl+D` | Tải ảnh lên kho R2, chép link vào clipboard, hiện thông báo "Đã chép link". |
| Lưu vào máy | `Ctrl+S` | Lưu PNG vào thư mục lưu, không hỏi lại. |
| Chép ảnh | `Ctrl+C` | Chép ảnh vào clipboard. |
| Ghim lên màn hình | `Ctrl+P` | Ghim vùng chọn lên màn hình thành một ảnh ghim. Xem "Ghim ảnh lên màn hình". |
| Nhận dạng chữ | `Ctrl+T` | Nhận dạng chữ trong vùng đã chọn và mở cửa sổ "Chữ nhận dạng". Xem mục "Nhận dạng chữ". |
| Thêm bước (chỉ khi có phiên) | `Ctrl+N` | Thêm vùng chọn (đã đánh dấu nếu có) làm một bước của phiên chụp từng bước. Lớp phủ đóng. Xem "Chụp từng bước". |
| Hủy | `Esc` | Đóng lớp phủ, không làm gì. |

- `Ctrl+D`, `Ctrl+S`, `Ctrl+C`, `Ctrl+P`, `Ctrl+T` chỉ có tác dụng khi đã có vùng chọn. Khi chưa có vùng chọn, các phím này không làm gì.
- `Ctrl+N` chỉ có tác dụng khi đã có vùng chọn và đang có phiên chụp từng bước. Khi không có phiên, phím này không làm gì và nút Thêm bước không hiện.
- `Esc` đóng lớp phủ bất cứ lúc nào, kể cả khi đang kéo khung chọn vùng. Khi đang có bảng mở, ô chữ, lần kéo hình hoặc hình được chọn, `Esc` chỉ lùi một bước (xem "Đánh dấu ảnh").

### Đánh dấu ảnh

Khi đã có vùng chọn và không đang kéo, một thanh công cụ dọc hiện cạnh vùng. Thanh nằm bên phải vùng, mép dưới thẳng mép dưới vùng. Không đủ chỗ bên phải thì sang bên trái; không đủ cả hai thì nằm trong vùng, sát mép phải. Thanh không chồng lên thanh công cụ ngang: nếu đẩy lên trên thanh ngang vẫn chồng thì thanh sang chỗ trống gần vùng nhất (dưới, phải hoặc trái thanh ngang, hoặc bên trái vùng).

Thanh có 9 mục, từ trên xuống: Chọn | Bút vẽ, Hình, Bút dạ quang, Làm mờ, Chữ, Đánh số | Cỡ, Màu. Vạch ngăn giữa các nhóm. Chú thích hiện khi rê chuột vào nút. Hoàn tác và Làm lại nằm ở đầu thanh công cụ ngang.

**Nút Hình** gom Đường thẳng, Mũi tên, Hình chữ nhật và Hình elip. Nút hiện biểu tượng của hình dùng gần nhất (mặc định Mũi tên), kèm một tam giác nhỏ ở góc dưới phải.

- Đang dùng công cụ khác (hoặc chưa chọn công cụ): bấm nút Hình là bật ngay hình dùng gần nhất, không mở bảng.
- Đang dùng một hình: bấm nút Hình mở bảng Hình (bốn ô, ô của hình đang dùng có viền sáng; rê chuột vào ô để xem tên và phím). Bấm một ô để bật hình đó và đóng bảng. Bấm lại nút Hình, bấm ra ngoài bảng hoặc bấm Esc thì chỉ đóng bảng.
- Phím `L`, `A`, `R`, `E` vẫn chọn thẳng từng hình và cũng đổi hình nhớ của nút. Hình nhớ được giữ trong lúc darkshot chạy (như màu và mức cỡ).
- Bảng Hình, bảng Cỡ và bảng màu không bao giờ mở cùng lúc.

Nếu thanh một cột cao hơn màn hình (trừ lề trên và dưới) thì thanh xếp thành hai cột: hết cột trái thì sang cột phải. Một cột cao 342 DIP, nên màn hình 1920×1080 ở DPI 200% vẫn đủ một cột; màn hình 1366×768 ở DPI 250% cần hai cột.

| Nút | Phím | Việc làm |
|---|---|---|
| Chọn | `V` | Chọn, di chuyển, đổi cỡ, xóa và đổi màu hay cỡ của từng hình đã vẽ. Xem "Sửa hình đã vẽ". |
| Bút vẽ | `P` | Kéo để vẽ nét tự do, nét 3 DIP ở mức Vừa. Bấm một cái tạo một chấm. |
| Hình: Đường thẳng | `L` | Kéo từ điểm đầu tới điểm cuối, nét 3 DIP ở mức Vừa. |
| Hình: Mũi tên | `A` | Như đường thẳng, có đầu mũi tên ở điểm cuối. |
| Hình: Hình chữ nhật | `R` | Kéo theo đường chéo, chỉ vẽ viền. |
| Hình: Hình elip | `E` | Kéo theo hộp như Hình chữ nhật, chỉ vẽ viền. Giữ Shift: hình tròn. |
| Bút dạ quang | `H` | Nét 16 DIP ở mức Vừa, trong mờ. Chỗ nét tự chồng lên nhau không đậm thêm. |
| Làm mờ | `B` | Tô kín vùng kéo bằng một màu trung bình của ảnh; không khôi phục được chữ bên dưới. |
| Chữ | `T` | Bấm để đặt ô chữ rồi gõ. |
| Đánh số | `N` | Bấm trong vùng chọn để đặt một vòng tròn có số tại điểm bấm. Số tự đánh lại 1, 2, 3… theo thứ tự vẽ. |
| Cỡ | `[` `]` | Mở bảng Cỡ (bốn mức). `[` giảm một mức, `]` tăng một mức. |
| Màu | | Mở bảng màu. |
| Hoàn tác (thanh ngang) | `Ctrl+Z` | Bỏ thay đổi gần nhất: thêm, xóa, di chuyển, đổi cỡ, đổi màu, đổi mức cỡ. Mờ khi không có gì để bỏ. |
| Làm lại (thanh ngang) | `Ctrl+Y` hoặc `Ctrl+Shift+Z` | Làm lại thay đổi vừa hoàn tác. Mờ khi không có gì để làm lại. |

DIP là đơn vị theo tỉ lệ hiển thị của Windows, nên nét trên màn hình có DPI cao vẫn cùng tỉ lệ với màn hình.

- Bấm một nút để chọn công cụ; bấm lại nút đang chọn để bỏ chọn. Mặc định không chọn công cụ nào: lớp phủ chạy như trước.
- Phím công cụ, `[`, `]`, Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z, Delete và Backspace chỉ có tác dụng khi đã có vùng chọn. Giữ phím lâu không đổi công cụ hay cỡ nhiều lần. Khi đang gõ chữ, các phím chữ đi vào ô chữ (xem mục Chữ).
- Khi đã chọn một công cụ vẽ (không tính Chọn): kéo bên trong vùng chọn là vẽ. Tay nắm ở viền và góc vẫn đổi cỡ vùng; kéo giữa vùng không di chuyển vùng. Con trỏ là chữ thập, và chữ I khi chọn Chữ.
- Giữ Shift khi kéo: Đường thẳng và Mũi tên khóa theo bước 45°; Hình chữ nhật thành hình vuông; Hình elip thành hình tròn.
- Kéo ngắn hơn 2 px với Đường thẳng, Mũi tên, Hình chữ nhật hoặc Hình elip thì không tạo hình.
- Đánh số: mỗi lần bấm trong vùng chọn đặt một vòng ngay tại điểm bấm, không cần kéo. Số của vòng là thứ tự của nó trong các vòng Đánh số, theo thứ tự vẽ. Xóa hoặc hoàn tác một vòng thì các số sau tự đánh lại, luôn liên tục từ 1.
- Khi đã có ít nhất một hình hoặc đang chọn công cụ, kéo ở ngoài vùng chọn không tạo vùng mới, để không mất hình đã vẽ. Muốn chọn lại từ đầu: hoàn tác hết rồi bỏ chọn công cụ.
- Hình vẽ kéo ra ngoài vùng vẫn được ghi nhưng chỉ hiện và in phần nằm trong vùng. Đổi cỡ hoặc di chuyển vùng (khi không có công cụ nào đang chọn) thì hình giữ nguyên vị trí trên màn hình: phần nằm ngoài vùng bị ẩn và không in; kéo vùng về thì hình hiện lại.

**Màu:** bấm nút Màu để mở bảng 48 màu cạnh nút (6 hàng × 8 cột như hộp chọn màu của Windows; mặc định đỏ `#FF0000`). Bấm một ô để chọn màu và đóng bảng. Bấm ra ngoài bảng hoặc bấm Esc thì chỉ đóng bảng, lớp phủ còn. Nếu đang chọn một hình, màu áp cho hình đó (một bước hoàn tác); hình không được chọn không đổi màu. Màu được nhớ trong lúc darkshot chạy; thoát rồi mở lại thì về đỏ.

Tự chọn màu: bảng mở ở dạng gọn ("Màu cơ bản", 16 ô "Màu tự chọn" và nút "Định nghĩa màu tự chọn >>"). Bấm một ô màu tự chọn đã có màu để dùng màu đó; ô trống thì không làm gì. Bấm "Định nghĩa màu tự chọn >>" để mở rộng bảng như hộp chọn màu của Windows: ô sắc độ (ngang là Hue, dọc là Sat), thanh độ sáng bên phải (Lum), ô xem màu và sáu ô số Sắc độ/Bão hòa/Độ sáng (thang 0–240 như Windows, Sắc độ 0–239) và Đỏ/Lục/Lam (0–255). Bấm hoặc kéo trong ô sắc độ và trên thanh độ sáng, hoặc bấm vào một ô số rồi gõ số (Tab sang ô kế, mũi tên lên/xuống tăng/giảm 1, Enter chốt ô; số quá khoảng được kẹp lại). Ở dạng đủ, bấm ô màu cơ bản hay ô tự chọn có màu chỉ lấy màu đó làm màu đang chỉnh; bấm một ô tự chọn để chọn nó làm ô đích (viền sáng). "Thêm vào màu tự chọn" ghi màu đang chỉnh vào ô đích (rồi ô đích sang ô kế), không có ô đích thì vào ô trống đầu tiên, hết ô trống thì ghi đè lần lượt từ ô đầu. "OK" hoặc Enter dùng màu đang chỉnh và đóng bảng; "Hủy", Esc hoặc bấm ra ngoài đóng bảng, màu vẽ không đổi. Khi đang sửa một ô số, Esc chỉ bỏ sửa ô đó. Lần mở bảng sau trong cùng lần chụp mở lại ở dạng đã dùng; lần chụp mới luôn mở dạng gọn. Màu tự chọn được lưu vào file cấu hình khi lớp phủ đóng, còn sau khi thoát và mở lại darkshot.

**Cỡ:** bấm nút Cỡ, hoặc dùng `[` (nhỏ hơn) và `]` (lớn hơn), để chọn một trong bốn mức: Nhỏ, Vừa (mặc định), Lớn, Rất lớn. Bảng Cỡ mở cạnh nút, bốn ô có chấm tròn lớn dần; ô đang dùng có viền sáng. Bấm một ô để chọn mức và đóng bảng; bấm ra ngoài bảng hoặc bấm Esc thì chỉ đóng bảng. Mở bảng Cỡ thì đóng bảng màu, và ngược lại. Mức áp cho hình vẽ sau đó; nếu đang chọn một hình thì áp cho hình đó. Làm mờ không đổi theo mức. Mức được nhớ trong lúc darkshot chạy (như màu), thoát rồi mở lại thì về Vừa.

| Mức | Nét: Bút vẽ, Đường thẳng, Mũi tên, Hình chữ nhật, Hình elip (DIP) | Bút dạ quang (DIP) | Chữ (DIP) | Bán kính vòng số (DIP) |
|---|---|---|---|---|
| Nhỏ | 2 | 10 | 14 | 10 |
| Vừa | 3 | 16 | 18 | 13 |
| Lớn | 5 | 24 | 26 | 17 |
| Rất lớn | 8 | 32 | 36 | 22 |

**Chữ:**

- Khi đang chọn Chữ, bấm trong vùng để đặt ô chữ tại đó. Ô đang gõ có khung nét đứt xanh và con trỏ nháy; khung và con trỏ không in ra ảnh.
- Enter xuống dòng. Chốt ô bằng cách bấm chỗ khác trong vùng, đổi công cụ, bấm nút hành động hoặc bấm Ctrl+D, Ctrl+S, Ctrl+C hoặc Ctrl+T. Ô rỗng khi chốt thì bỏ.
- Gõ được Backspace, Delete, mũi tên bốn hướng, Home và End. Ctrl+V dán chữ thuần: xuống dòng giữ nguyên, Tab thành khoảng trắng. Không chọn chữ bằng chuột hay Shift.
- Gõ được tiếng Việt bằng Unikey, EVKey hoặc bộ gõ tiếng Việt của Windows. Khi đang gõ, mọi phím chữ (kể cả V, P, L, A, R, E, H, B, T, N, `[` và `]`) đi vào ô chữ, không đổi công cụ hay cỡ.
- Chữ có viền đen hoặc trắng (theo màu chữ) để đọc được trên mọi nền.
- Ctrl+Z khi ô chữ còn nội dung (kể cả chỉ có khoảng trắng) thì xóa ô đó trước; ô rỗng thì hoàn tác thay đổi trước đó. Khi ô chữ đang mở, Ctrl+Y và Ctrl+Shift+Z không làm gì.
- Esc khi đang gõ hủy ô chữ, không chốt. Khi bộ gõ đang soạn dở một chữ, Esc thuộc về bộ gõ.

**Sửa hình đã vẽ:** chọn công cụ Chọn (`V`) để chọn, di chuyển, đổi cỡ, xóa và đổi màu hay mức cỡ của từng hình. Chọn công cụ khác thì bỏ chọn hình.

- Bấm một hình để chọn; nếu các hình chồng nhau thì hình trên cùng được chọn. Kéo hình đã chọn để di chuyển. Cả một cú bấm-kéo là một bước hoàn tác; bấm mà không kéo không tạo bước nào.
- Hình được chọn có khung xanh quanh hộp bao và tay nắm vuông. Đường thẳng và Mũi tên chỉ có hai tay nắm ở hai đầu, không có khung. Khung và tay nắm chỉ hiện trên màn hình, không bao giờ in vào ảnh.
- Kéo tay nắm để đổi cỡ: Hình chữ nhật, Hình elip và Làm mờ có tám tay nắm (góc và cạnh); Bút vẽ và Bút dạ quang có bốn tay nắm ở góc hộp; Chữ và Đánh số có bốn tay nắm ở góc; Đường thẳng và Mũi tên có hai tay nắm ở hai đầu. Kéo qua cạnh đối diện thì hình lật lại. Giữ Shift khi kéo góc của hình chữ nhật, elip, làm mờ, bút vẽ hoặc bút dạ quang để giữ tỉ lệ; giữ Shift khi kéo đầu Đường thẳng hay Mũi tên để khóa góc 45°.
- Chữ đổi cỡ chữ khi kéo góc; Đánh số đổi bán kính vòng khi kéo góc.
- Làm mờ khi dời hoặc đổi cỡ: màu che được tính lại từ ảnh chụp gốc nên vùng che vẫn kín. Nếu vùng che được dời đi, phần ảnh gốc bên dưới lộ ra và thấy ngay trên màn hình, đúng như ảnh ra. Hoàn tác hoặc làm lại trả lại đúng màu đã lưu.
- Delete hoặc Backspace xóa hình đang chọn (khi không đang gõ chữ). Xóa cũng hoàn tác được.
- Khi đang chọn một hình, chọn màu hoặc mức trong bảng Cỡ, hoặc dùng `[` `]`, thì áp cho hình đó. Làm mờ không đổi màu hay mức.
- Bấm chỗ khác (trong hoặc ngoài vùng chọn, không phải tay nắm) thì bỏ chọn hình; không tạo vùng chọn mới.
- Hoàn tác và làm lại áp cho mọi thao tác này. Sau hoàn tác hoặc làm lại, hình không còn được chọn.

**Esc** lùi từng bước theo thứ tự: đóng bảng Cỡ, bảng Hình hoặc bảng màu đang mở; đang gõ chữ thì hủy ô chữ (khi bộ gõ đang soạn dở một chữ, Esc thuộc về bộ gõ); đang kéo một hình thì hủy lần kéo, hình về như cũ, không ghi gì; đang chọn một hình thì bỏ chọn; còn lại thì đóng lớp phủ và bỏ mọi nét, như trước.

**In vào ảnh:** ảnh tải lên, ảnh lưu vào máy và ảnh chép đều có các nét đã vẽ, ở đúng chỗ và đúng màu như trên màn hình. Ảnh không có nét thì giữ như trước. Khung chọn, tay nắm và khung ô chữ không bao giờ có trong ảnh. Nếu không in được nét, darkshot báo "Không xử lý được ảnh" và dừng việc lưu, chép hoặc tải lên, để không có ảnh thiếu nét. Trong trường hợp đó hãy chụp lại.

Hai phím toàn màn hình (Lưu ảnh và Tải lên nhanh, mục "Phím tắt chụp") không mở lớp phủ nên không có thanh này. Nét vẽ không được nhớ sang lần chụp sau.

Ngoài phạm vi của bản này: chọn nhiều hình cùng lúc, khung chọn kéo nhiều hình, đổi thứ tự trên dưới (hình vẽ sau luôn nằm trên hình vẽ trước), xoay, nhân bản, chép và dán hình, sửa nội dung chữ đã chốt (chỉ dời, đổi cỡ, đổi màu), di chuyển hình bằng phím mũi tên, nút Xóa trên thanh công cụ (dùng Delete hoặc Backspace), và lưu mức cỡ vào file cấu hình (mức chỉ nhớ trong lúc darkshot chạy).

### Lưu vào máy

- Định dạng PNG, thư mục mặc định `Pictures\darkshot` (đổi được trong Tùy chọn, tab Thông thường; thư mục chưa có thì được tạo).
- Tên file: `darkshot_YYYY-MM-DD_HH-MM-SS.png` theo giờ máy. Nếu trùng tên thì thêm `_2`, `_3`.
- Ảnh lưu vào máy luôn là PNG, kể cả khi chọn JPEG cho ảnh tải lên và kể cả bản lưu dự phòng sau khi tải lên thất bại.

### Tải lên

- Ảnh được đưa vào thư mục hạn trên kho. Link có dạng `{địa chỉ công khai}/{thư mục hạn}/{tên}.png`, ví dụ `https://img.example.com/d30/k3X9f2abQ7mZp1Lw.png`. Khi chọn JPEG trong tab Định dạng, đuôi là `.jpg`.
- Tên ảnh gồm 16 ký tự chữ và số, sinh từ bộ sinh số ngẫu nhiên của Windows.
- Tải lên thành công: link được chép vào clipboard, thông báo "Đã chép link" (có thể tắt, xem mục "Thông báo").
- Tải lên thất bại (mất mạng, khóa sai, kho không tồn tại, địa chỉ công khai không mở được): ảnh vẫn được lưu vào thư mục lưu. Thông báo "Không tải lên được" nêu lý do và đường dẫn ảnh đã lưu.
- Chưa cấu hình R2 mà bấm tải lên: ảnh được lưu vào thư mục lưu, thông báo nói rõ chưa cấu hình, và cửa sổ Tùy chọn tự mở ở tab Lưu trữ R2.

### Hạn của link

Mục **Hạn của link** nằm trong tab **Lưu trữ R2** của Tùy chọn, có bốn lựa chọn:

| Lựa chọn | Thư mục trên kho | Ghi chú |
|---|---|---|
| 1 ngày | `d1/` | |
| 7 ngày | `d7/` | |
| 30 ngày | `d30/` | Mặc định. |
| Không hết hạn | `keep/` | Không có quy tắc xóa cho thư mục này. |

Hạn áp dụng cho mọi lần tải lên sau khi bấm OK. Không chọn hạn riêng cho từng ảnh.

### Tùy chọn

Cửa sổ Tùy chọn có bốn tab:

- **Thông thường:** **Khởi động cùng Windows**, bốn ô tích **Hiện các thông báo lưu và sao chép**, **Tự kiểm tra bản mới**, **Giữ vị trí vùng đã chọn**, **Bao gồm con trỏ chuột vào ảnh chụp màn hình**, Thư mục lưu (nút **Chọn…**), Ngôn ngữ.
- **Phím tắt:** bốn dòng phím tắt, mỗi dòng có ô tích bật/tắt, ô phím chỉ đọc và nút **Đổi…** (xem mục "Phím tắt chụp").
- **Định dạng:** hai nút chọn **PNG** (mặc định) và **JPEG**, thanh trượt **Chất lượng JPEG** từ 50 đến 100 (mặc định 90). Thanh trượt và số mờ khi chọn PNG. Chỉ áp dụng cho ảnh tải lên; ảnh lưu vào máy luôn là PNG. JPEG vẫn là nén có mất mát, kể cả ở mức 100.
- **Lưu trữ R2:** Account ID, Access Key ID, Secret Access Key, Tên kho (bucket), Địa chỉ công khai, Hạn của link, nút **Kiểm tra kết nối**.

Nút **OK** ghi cài đặt rồi đóng cửa sổ. Nút **Hủy** bỏ thay đổi. Nút **Trợ giúp** mở cửa sổ Trợ giúp và không đóng Tùy chọn.

- **Địa chỉ công khai** phải bắt đầu bằng `https://`, không có khoảng trắng. Dấu `/` cuối được bỏ tự động khi lưu.
- **Secret Access Key** không hiện lại giá trị đã lưu. Ô hiện dòng "Đã lưu khóa; để trống nếu không đổi." Để trống ô này khi bấm OK thì khóa cũ vẫn được dùng.
- Ô sai (ví dụ địa chỉ công khai sai, thư mục lưu trống) làm OK không đóng cửa sổ. Cửa sổ chuyển đến tab chứa ô đó và báo lỗi ngay trong cửa sổ.

### Ngôn ngữ

Giao diện có hai ngôn ngữ: Tiếng Việt và English. Đổi ở Tùy chọn, tab Thông thường, mục Ngôn ngữ; có hiệu lực ngay khi bấm OK.

- Khi chưa có file cấu hình (lần chạy đầu) hoặc file cấu hình bị hỏng: theo ngôn ngữ giao diện của Windows. Windows dùng tiếng Việt thì là Tiếng Việt, còn lại là English.
- File cấu hình của bản cũ không có mục ngôn ngữ: Tiếng Việt.
- **Kiểm tra kết nối** tải lên một file thử nhỏ, tải lại qua địa chỉ công khai để chắc link mở được, rồi xóa file thử. Kết quả hiện ngay bên cạnh nút. Thông báo lỗi phân biệt lỗi tải lên với lỗi địa chỉ công khai. File thử nằm ở `d1/darkshot-test-{16 ký tự}.txt`. Nếu bước xóa thất bại, kết quả báo lỗi quyền dù tải lên đã được, và file thử ở lại trên kho đến khi quy tắc `d1/` xóa nó.

### Phím tắt chụp

Tab **Phím tắt** của Tùy chọn có bốn dòng. Mỗi dòng có ô tích bật/tắt, ô phím chỉ đọc và nút **Đổi…**.

| Dòng | Mặc định | Việc làm |
|---|---|---|
| Chụp vùng màn hình | Bật, `PrintScreen` | Mở lớp phủ để chọn vùng. |
| Lưu ảnh (toàn màn hình) | Tắt, `Shift+PrintScreen` | Lưu PNG của màn hình đang có con trỏ chuột, không mở lớp phủ. |
| Tải lên nhanh (toàn màn hình) | Tắt, `Ctrl+PrintScreen` | Tải lên ảnh của màn hình đang có con trỏ chuột và chép link, không mở lớp phủ. |
| Chụp lại vùng trước | Tắt, `Ctrl+Shift+PrintScreen` | Chụp lại đúng vùng của lần chụp vùng trước và làm lại hành động đã chọn (Lưu, Chép, Tải lên hoặc Ghim), không mở lớp phủ. Xem "Chụp lại vùng trước". |

- "Toàn màn hình" là màn hình đang có con trỏ chuột lúc bấm phím. Nếu con trỏ không nằm trên màn hình nào thì lấy màn hình gần nhất. Ảnh là cả màn hình: không có vùng chọn và không có thanh công cụ.
- Hai phím toàn màn hình không đổi vùng đã nhớ của mục "Giữ vị trí vùng chọn", và không đổi lần chụp trước của "Chụp lại vùng trước".
- Bỏ tích một dòng thì phím của dòng đó không còn được đăng ký; ô phím và nút **Đổi…** mờ đi. Tổ hợp của dòng vẫn được giữ, tích lại là dùng được.
- Các dòng đang bật không được trùng tổ hợp: bấm OK khi trùng thì cửa sổ báo lỗi và không đóng. Dòng đang tắt có thể trùng với dòng khác.
- Để đổi: bấm **Đổi…** rồi nhấn tổ hợp mới. Bấm **Esc** để bỏ qua việc đổi.
- Phím thường phải kèm **Ctrl**, **Alt** hoặc **Win**. Ví dụ `Ctrl+Shift+S` được nhận, còn chữ `S` một mình thì không.
- Các phím được đứng riêng: `PrintScreen`, `Pause`, `ScrollLock`, `F1` đến `F24`.
- Shift một mình không đủ. `Alt+F4` vẫn là lệnh đóng cửa sổ của Windows.
- Nếu tổ hợp đang bị ứng dụng khác giữ (ví dụ Lightshot giữ `PrintScreen`), Tùy chọn báo "... đang được ứng dụng khác dùng. Hãy chọn phím khác." và không đóng cửa sổ. Ứng dụng vẫn chạy, và vẫn chụp được qua menu khay.
- Khi khởi động, dòng bật nào không đăng ký được thì biểu tượng khay hiện thông báo "Phím tắt chưa hoạt động", nêu tổ hợp bị chiếm và gợi ý mở Tùy chọn. Mọi dòng lỗi gom vào một thông báo.

### Thông báo

- Ô **Hiện các thông báo lưu và sao chép** (mặc định bật) quyết định ba thông báo thành công: "Đã lưu ảnh", "Đã chép ảnh" và "Đã chép link". Tắt ô này thì ba thông báo đó không hiện.
- Thông báo lỗi luôn hiện, kể cả khi tắt ô.

### Giữ vị trí vùng chọn

- Bật **Giữ vị trí vùng đã chọn** (mặc định tắt): lần chụp vùng sau mở lớp phủ với khung của lần trước, đã chọn sẵn, có tay nắm, nhãn kích thước và thanh công cụ.
- Khung được nhớ khi lượt chụp vùng kết thúc bằng Lưu, Chép, Tải lên hoặc Ghim. Bấm Esc thì giữ khung cũ.
- Khung vẫn được nhớ khi ô đang tắt, nên bật ô lên là dùng được ngay.
- Khung chỉ dùng lại khi nằm trọn trong ảnh chụp hiện tại và chạm ít nhất một màn hình đang có. Nếu không, lớp phủ mở trống.
- Chỉ nhớ tọa độ, không lưu ảnh. Vị trí được nhớ qua khởi động lại, cùng tệp với mục "Chụp lại vùng trước": thoát rồi mở lại vẫn chọn sẵn khung cũ (nếu khung còn nằm trên màn hình).

### Chụp lại vùng trước

Phím **Chụp lại vùng trước** (mặc định tắt, `Ctrl+Shift+PrintScreen`; bật ở Tùy chọn, tab Phím tắt) hoặc mục **Chụp lại vùng trước** ở menu khay (ngay sau Chụp màn hình) chụp lại đúng vùng của lần chụp vùng gần nhất và làm lại đúng hành động đã chọn: Lưu, Chép, Tải lên hoặc Ghim.

- Không mở lớp phủ và không chọn lại vùng. Không lặp các nét đánh dấu: ảnh chỉ là vùng màn hình, không có hình đã vẽ.
- Vùng là tọa độ trên màn hình. Lần chụp lặp lại không đổi "lần chụp trước": lần sau vẫn lặp vùng cũ.
- Chụp toàn màn hình (`Shift+PrintScreen`, `Ctrl+PrintScreen`) không đổi lần chụp trước.
- Mục ở menu khay mờ khi chưa có lần chụp vùng nào. Khi phím được bấm mà chưa có lần chụp, lớp phủ mở như phím chụp vùng (kể cả gợi ý cửa sổ).
- Lần chụp được nhớ khi lượt chụp vùng kết thúc bằng Lưu, Chép, Tải lên hoặc Ghim. Bấm Esc (hủy) thì không ghi gì.
- Nhớ qua khởi động lại: tệp `%APPDATA%\darkshot\last_capture.json` chỉ chứa tọa độ vùng và tên hành động (`save`, `copy`, `upload` hoặc `pin`). Tệp không chứa ảnh, nhãn hay nét vẽ. Tệp hỏng thì được coi như chưa có lần chụp, và không bị ghi đè cho đến lần chụp vùng kết thúc bằng hành động.
- Nếu vùng không còn trên màn hình (ví dụ đã rút màn hình hoặc đổi độ phân giải), lớp phủ mở trên ảnh chụp hiện tại để chọn lại. Sau khi lớp phủ đóng, darkshot báo "Vùng trước không còn trên màn hình. Hãy chọn lại."

### Con trỏ chuột trong ảnh
### Con trỏ chuột trong ảnh

- Bật **Bao gồm con trỏ chuột vào ảnh chụp màn hình** (mặc định tắt): con trỏ được vẽ vào ảnh ngay lúc chụp, nên ảnh nền của lớp phủ cũng có con trỏ đứng yên.
- Con trỏ đang ẩn thì không được vẽ.
- Con trỏ được vẽ ở cỡ mà Windows trả về lúc chụp. Trên màn hình có DPI khác 100%, hoặc khi con trỏ được phóng to trong Cài đặt Windows, con trỏ trong ảnh có thể nhỏ hơn trên màn hình.

### Gợi ý cửa sổ

Mặc định **bật** (Tùy chọn, tab Thông thường, ô "Gợi ý cửa sổ khi chọn vùng").

- Khi lớp phủ chưa có vùng chọn, khung của cửa sổ dưới con trỏ sáng lên kèm nhãn kích thước; rê ra nền thì cả màn hình đó sáng lên. Bấm một lần (không kéo) hoặc Enter để chọn đúng khung, sau đó đánh dấu và Lưu/Chép/Tải lên như vùng kéo tay.
- Kéo chuột vẫn chọn vùng tự do như trước; đã có vùng chọn (kể cả vùng nhớ từ lần trước) thì không có gợi ý.
- Viên nhắc ở mép trên màn hình chứa con trỏ: "Bấm: chọn cửa sổ · Kéo: chọn vùng · Esc: thoát"; tự ẩn khi con trỏ tới gần, khi kéo và khi đã có vùng chọn. Viên nhắc không nằm trong ảnh.
- Khung dùng biên DWM (không có viền đổi cỡ vô hình 7 px). Bỏ qua cửa sổ ẩn, thu nhỏ, ở màn hình ảo khác, cửa sổ trong suốt với chuột hoặc độ mờ 0, và nền màn hình.
- darkshot chỉ đọc khung (tọa độ) và tên lớp cửa sổ lúc chụp, không đọc tiêu đề hay nội dung, không lưu, không gửi đi.
- Tắt ô này thì lớp phủ chạy như bản trước, viên nhắc rút gọn "Kéo để chọn vùng · Esc để thoát".

### Lịch sử ảnh chụp

Mặc định **tắt**. Bật ở Tùy chọn, tab Thông thường, ô "Bật lịch sử ảnh chụp". Khi bật, mỗi lần Lưu, Chép hoặc Tải lên (kể cả lưu dự phòng khi tải lên không được) thêm một mục vào lịch sử; khi tắt, không có gì được ghi thêm và các mục cũ giữ nguyên.

- Mở từ menu khay, mục "Lịch sử ảnh chụp". Danh sách bên trái (ảnh nhỏ, ngày giờ, loại, kích thước), khung xem trước và hành động bên phải.
- Hành động: Mở (ảnh đã lưu mở bằng trình xem ảnh; ảnh đã tải lên mở liên kết `https://` trên trình duyệt), Sao chép ảnh, Sao chép liên kết (ảnh đã tải lên), Mở thư mục chứa ảnh (ảnh đã lưu), Ghim (mở ảnh ghim giữa màn hình, xem "Ghim ảnh lên màn hình"), Xóa khỏi lịch sử. Chuột phải trên một dòng cho cùng các mục. Phím: Enter mở, Delete xóa mục, Ctrl+C chép ảnh, Ctrl+Shift+C chép liên kết, Ctrl+F tìm, Esc đóng.
- Tìm theo tên tệp, ngày, loại hoặc liên kết (không phân biệt hoa thường và dấu); lọc Tất cả, Đã lưu, Đã tải lên, Đã chép.
- Giữ 50 mục gần nhất. Dữ liệu nằm ở `%LOCALAPPDATA%\darkshot\history\` (chỉ mục `index.json`, ảnh nhỏ, và bản sao PNG của ảnh đã tải lên hoặc đã chép). Ảnh đã lưu chỉ được tham chiếu, không chép lại. Không gửi đi đâu.
- Xóa một mục hoặc "Xóa lịch sử" chỉ xóa phần do ứng dụng giữ. Ảnh bạn đã lưu và ảnh trên R2 không bị xóa.
- Ghi lịch sử chạy trên một luồng nền theo thứ tự; khi thoát (kể cả tắt máy), darkshot chờ tối đa 3 giây để ghi xong các mục còn dở, nên thoát ngay sau Lưu/Chép/Tải lên vẫn giữ được mục. Một lượt tải lên chỉ tạo đúng một mục, kể cả khi thoát đúng lúc nó vừa xong.
- Hạn hiển thị của ảnh đã tải lên tính từ lúc tải xong (R2 tính vòng đời từ lúc tạo đối tượng) và là ngày "còn ít nhất tới": R2 có thể xóa trễ tới khoảng một ngày.

### Ghim ảnh lên màn hình

Ghim đặt một ảnh chụp lên trên các cửa sổ khác. Mỗi ảnh ghim là một cửa sổ riêng, luôn nằm trên cùng. Tối đa 32 ảnh ghim cùng lúc.

Có hai cách ghim:

- **Từ lớp phủ:** sau khi chọn vùng (và đánh dấu nếu cần), bấm nút **Ghim** (biểu tượng ghim) trên thanh công cụ ngang, hoặc `Ctrl+P`. Ảnh ghim đặt khít đúng vị trí vùng chọn trên màn hình và có các nét đã vẽ. Lớp phủ đóng như khi Lưu hoặc Chép.
- **Từ Lịch sử:** chọn một ảnh trong Lịch sử ảnh chụp rồi bấm **Ghim** ở khung chi tiết. Ảnh hiện giữa màn hình đang có con trỏ chuột, và không lớn hơn 80% vùng làm việc của màn hình đó.

Thao tác trên ảnh ghim:

- **Kéo** để dời ảnh. Ảnh luôn còn một phần nằm trên một màn hình.
- **Cuộn chuột** để phóng hoặc thu quanh con trỏ: điểm dưới con trỏ đứng yên. Nhãn mức (ví dụ `125%`) hiện khoảng 1,2 giây.
- **Ctrl+cuộn** để đổi độ mờ (từ 20% trở lên). Nhãn "Độ mờ 60%" hiện khoảng 1,2 giây.
- **Bấm đúp** để về 100%. Bấm đúp trên nút × thì ảnh không phóng.
- **Chuột phải** mở menu: Sao chép ảnh, Lưu ảnh, Nhận dạng chữ, Thu phóng, Độ mờ, Đóng. Mức đang dùng có dấu tích.
- **Rê chuột vào ảnh** để hiện nút ×; bấm nút × để đóng ảnh đó.
- **Phím:** `+` và `-` phóng hoặc thu theo bậc (từ 10% đến 400%); `0` về 100%; mũi tên dời ảnh 1 px, Shift+mũi tên 10 px; `Esc` hoặc `Ctrl+W` đóng ảnh; `Ctrl+C` chép ảnh; `Ctrl+S` lưu ảnh.

Giới hạn phóng: mỗi cạnh hiển thị tối đa 16.384 px. Khi phóng trên 100%, diện tích hiển thị tối đa 40 triệu px; ví dụ ảnh 4K (3840×2160) phóng tối đa 200%.

- **Menu khay:** khi đang có ảnh ghim, menu khay có mục "Đóng mọi ảnh ghim (n)" với n là số ảnh đang ghim; bấm để đóng hết. Khi không có ảnh ghim, mục này không hiện.
- **Đủ 32 ảnh:** ghim thêm thì hiện "Đã đủ ảnh ghim" và nêu số 32. Đóng bớt ảnh rồi thử lại.
- **Không tạo được cửa sổ:** hiện "Không ghim được ảnh", không kèm lý do kỹ thuật. Thử lại.
- Thoát darkshot đóng mọi ảnh ghim.

Chạy thử cách ly (dữ liệu tổng hợp, không dùng cấu hình hay khóa R2 thật, không mạng):

```powershell
powershell -File scripts/demo-pin.ps1 -Exe target\demo9\release\darkshot.exe
```

Thêm `-Lang en` để chạy giao diện English. Chụp vùng: `Ctrl+Alt+Shift+F10`. Cửa sổ Lịch sử mở sẵn. Danh sách kiểm tra của đợt: [docs/testing/manual-checklist.md](docs/testing/manual-checklist.md), mục "Đợt 9".

### Nhận dạng chữ

Nhận dạng chữ trong vùng đã chọn (hoặc trong ảnh ghim) và đưa chữ vào cửa sổ **Chữ nhận dạng**. Nhận dạng chạy hoàn toàn trên máy bằng OCR của Windows: không dùng mạng, ảnh và chữ không gửi đi đâu, không ghi ra file, không vào Lịch sử, và không đổi "lần chụp trước".

- **Từ lớp phủ:** chọn vùng, rồi bấm nút **Nhận dạng chữ** (giữa Ghim và Hủy) hoặc `Ctrl+T`. Khi chưa có vùng chọn thì `Ctrl+T` không làm gì. Đang gõ dở một ô chữ thì ô đó được chốt trước. Ảnh đem nhận dạng gồm cả các nét đã vẽ, nên vùng đã làm mờ sẽ không đọc được chữ bên dưới.
- **Từ ảnh ghim:** chuột phải, mục **Nhận dạng chữ** (ngay sau Lưu ảnh).
- **Cửa sổ Chữ nhận dạng:** hiện ngay "Đang nhận dạng chữ…", rồi hiện chữ trong một ô sửa được. Dòng trạng thái ghi tên ngôn ngữ và số dòng, ví dụ "Ngôn ngữ: English (United States) · 2 dòng".
- **Chép** (`Ctrl+Enter`, hoặc `Enter` khi con trỏ không ở ô chữ) chép chữ đang có trong ô vào clipboard, hiện "Đã chép chữ", rồi đóng cửa sổ. **Đóng** hoặc `Esc` đóng cửa sổ, không chép gì. Cỡ cửa sổ đổi được.
- Tối đa 8 cửa sổ chữ cùng lúc. Mở thêm khi đã đủ 8 thì cửa sổ cũ nhất đóng.
- Không tìm thấy chữ: hiện "Không tìm thấy chữ trong ảnh."

Ngôn ngữ:

- Darkshot nhận dạng theo danh sách ngôn ngữ trong cài đặt Windows của bạn (Cài đặt > Thời gian & ngôn ngữ > Ngôn ngữ & khu vực).
- Tiếng Việt chỉ nhận dạng được nếu Windows có gói nhận dạng tiếng Việt. Máy không có gói đó thì chữ Việt không được nhận ra.
- Máy chưa có gói nhận dạng nào: hiện câu hướng dẫn thêm ngôn ngữ trong Cài đặt Windows.

Chạy thử cách ly (dữ liệu tổng hợp, không dùng cấu hình hay khóa R2 thật, không mạng):

```powershell
powershell -File scripts/demo-ocr.ps1 -Exe target\demo11\release\darkshot.exe
```

Thêm `-Lang en` để chạy giao diện English. Script mở sẵn một file mẫu có chữ trong Notepad: chụp vùng đó bằng `Ctrl+Alt+Shift+F10`, rồi bấm `Ctrl+T`. Danh sách kiểm tra của đợt: [docs/testing/manual-checklist.md](docs/testing/manual-checklist.md), mục "Đợt 11".

### Chụp từng bước

Gom nhiều lần chụp vùng thành một **phiên**: mỗi bước là một ảnh có hình đánh dấu, được đánh số tự động, có thể ghi chú rồi xuất cả bộ ra một thư mục gồm ảnh đánh số và tệp `steps.md`. Tối đa 99 bước. Phiên chạy hoàn toàn trên máy, không dùng mạng.

- **Bắt đầu:** menu khay, mục **Chụp từng bước** (ngay sau Lịch sử ảnh chụp). Khi đã có phiên, mục này đổi thành **Xem lại các bước (N)**, với N là số bước.
- **Thanh phiên:** cửa sổ nhỏ, tối, luôn nằm trên cùng, không lấy tiêu điểm, mặc định ở giữa mép trên của màn hình chính. Kéo được bằng vùng chữ. Thanh hiện chấm đỏ và "Chụp từng bước · N bước", kèm ba nút: **Chụp bước** (chụp vùng như phím chụp), **Xem lại** và **Kết thúc**. Trên Windows 10 2004 trở lên, thanh không xuất hiện trong ảnh chụp. Máy cũ hơn thì thanh tự ẩn trong lúc chụp rồi hiện lại.
- **Thêm bước:** khi có phiên, lớp phủ có thêm nút **Thêm bước** (`Ctrl+N`) ngay trước Hủy. Ảnh bước là vùng chọn có hình đánh dấu. Lớp phủ đóng và thanh phiên cập nhật số bước. Bước không ghi vào Lịch sử ảnh chụp và không đổi "lần chụp trước".
- **Nơi lưu:** `%LOCALAPPDATA%\darkshot\steps\current\`, gồm `001.png`, `002.png`… (tên theo thứ tự thêm, không đổi khi sắp xếp lại) và `session.json`. Ảnh bước chỉ nằm trên máy này cho đến khi bạn xuất.
- **Khôi phục:** thoát darkshot rồi mở lại thì phiên tiếp tục, đủ bước và ghi chú. Nếu `session.json` hỏng, thư mục được đổi tên thành `current.corrupt-<thời gian>` để giữ lại ảnh, và darkshot bắt đầu không có phiên. Tệp PNG bị thiếu thì bước đó bị bỏ khỏi danh sách.
- **Cửa sổ Các bước** (nút **Xem lại** trên thanh phiên, hoặc mục menu): bên trái là danh sách bước (ảnh nhỏ, "Bước k", dòng đầu của ghi chú); bên phải là xem trước và ô **Ghi chú** (tối đa 2000 ký tự, lưu khi rời ô, đổi bước hoặc đóng). Nút **Lên**, **Xuống**, **Xóa bước** (phím Delete; xóa ngay, không hỏi, và xóa tệp ảnh của bước đó), **Xuất…**, **Kết thúc phiên**, **Đóng** (phím Esc). Khi chưa có bước nào, cửa sổ hiện "Chưa có bước nào. Bấm Chụp bước trên thanh phiên."
- **Xuất…:** hộp chọn thư mục hiện ra, mặc định là thư mục lưu ảnh. darkshot tạo thư mục con `darkshot-steps-YYYY-MM-DD_HH-mm` (thêm `_2`, `_3`… nếu trùng; không ghi vào thư mục đã có). Trong đó có `01.png`, `02.png`… theo thứ tự hiển thị và `steps.md` gồm tiêu đề, ngày, "Bước k", ghi chú và ảnh của từng bước. Ký tự Markdown trong ghi chú được thoát. Xong thì hiện "Đã xuất N bước" và mở thư mục trong Explorer. Xuất không kết thúc phiên, và phiên giữ nguyên nếu ghi thất bại.
- **Kết thúc phiên** (trên thanh phiên hoặc trong cửa sổ Các bước): nếu còn bước chưa xuất, hoặc đã đổi gì sau lần xuất cuối, darkshot hỏi "Kết thúc mà không xuất? N bước sẽ bị xóa." Bấm **Kết thúc** để xóa thư mục phiên, hoặc **Hủy** để giữ lại. Đã xuất rồi và không đổi gì sau đó, hoặc không có bước nào, thì kết thúc ngay.

Chạy thử cách ly (dữ liệu tổng hợp 3 bước, không dùng cấu hình hay khóa R2 thật, không mạng):

```powershell
powershell -File scripts/demo-steps.ps1 -Exe target\demo12\release\darkshot.exe
```

Thêm `-Lang en` để chạy giao diện English. Khi darkshot khởi động, thanh phiên hiện ngay với 3 bước mẫu (phiên được khôi phục từ thư mục cách ly). Danh sách kiểm tra của đợt: [docs/testing/manual-checklist.md](docs/testing/manual-checklist.md), mục "Đợt 12".

### Menu khay

| Mục | Việc làm |
|---|---|
| Chụp màn hình | Mở lớp phủ chụp. |
| Chụp lại vùng trước | Chụp lại đúng vùng của lần chụp vùng gần nhất và làm lại hành động đã chọn, không mở lớp phủ. Mờ khi chưa có lần chụp vùng nào. Xem "Chụp lại vùng trước". |
| Lịch sử ảnh chụp | Mở cửa sổ Lịch sử (xem mục "Lịch sử ảnh chụp"). Nếu đã mở thì đưa ra trước. |
| Chụp từng bước | Bắt đầu một phiên và mở thanh phiên (xem "Chụp từng bước"). Khi đã có phiên, mục là **Xem lại các bước (N)**: mở cửa sổ Các bước. |
| Tùy chọn... | Mở cửa sổ Tùy chọn. Nếu đã mở thì đưa cửa sổ đó ra trước. |
| Thông tin... | Mở cửa sổ Thông tin: biểu tượng, tên, phiên bản và mô tả ngắn. |
| Trợ giúp | Mở cửa sổ Trợ giúp: các bước dùng darkshot và chuẩn bị Cloudflare R2, theo phím tắt đang đặt. |
| Thoát | Thoát ứng dụng và xóa biểu tượng khỏi khay. |

Thông tin và Trợ giúp không chặn ứng dụng: lúc hai cửa sổ này mở, vẫn chụp, dùng menu và mở Tùy chọn được. Mở lại khi đang mở thì chỉ đưa cửa sổ ra trước. Đóng bằng Enter, Esc, nút X hoặc nút Đóng.

Chỉ một bản darkshot chạy cùng lúc. Mở bản thứ hai thì bản mới thoát ngay.

## Giao diện

- darkshot luôn dùng giao diện tối, không theo chế độ sáng hoặc tối của Windows.
- Thông báo ở khay và hộp chọn thư mục vẫn theo chế độ của Windows, sáng hoặc tối.
- Thanh tiêu đề tối cần Windows 10 bản 20H1 trở lên. Bản cũ hơn vẫn chạy, với thanh tiêu đề sáng.
- Menu khay dùng nền tối khi Windows cho phép. Nếu không, menu vẫn chạy với nền sáng.
- Biểu tượng: nền than chì, viền xám nhạt, bốn góc khung màu xanh `#4C8DFF`, chấm sáng ở giữa (cỡ 16 đến 24 px không có chấm). Nếu khay, thanh tác vụ hoặc Explorer vẫn hiện biểu tượng xanh cũ sau khi cập nhật, khởi động lại Explorer (Task Manager, **Windows Explorer**, **Restart**) để xóa bộ đệm biểu tượng.
- Ô tích, nút chọn, thanh trượt, ô nhập và hộp chọn trong Tùy chọn được vẽ lại theo màu tối, cạnh khử răng cưa; bàn phím, Tab và Trình tường thuật vẫn dùng như điều khiển gốc của Windows.

## Chuẩn bị Cloudflare R2

Các bước dưới đây theo tài liệu R2 của Cloudflare, đối chiếu ngày 2026-11-16. Giao diện Cloudflare có thể đổi tên nút. Nếu tên không khớp, hãy tìm theo mô tả trong tài liệu.

Tài liệu tham khảo:

- Tạo bucket: https://developers.cloudflare.com/r2/buckets/create-buckets/
- Truy cập công khai: https://developers.cloudflare.com/r2/buckets/public-buckets/
- API token: https://developers.cloudflare.com/r2/api/tokens/
- Quy tắc vòng đời: https://developers.cloudflare.com/r2/buckets/object-lifecycles/

### Bước 1: Tạo tài khoản và bucket

1. Tạo tài khoản Cloudflare, đăng nhập trang quản lý, rồi mở mục **R2 object storage**.
2. Tạo một bucket (kho) mới. Tên bucket chỉ gồm chữ thường `a-z`, số `0-9` và dấu gạch ngang `-`, dài từ 3 đến 63 ký tự, không bắt đầu hoặc kết thúc bằng `-`. Đây là quy tắc của Cloudflare; darkshot không tự kiểm độ dài hay chữ hoa.
3. Ghi lại **Account ID** (hiện trong trang tổng quan R2). Đây là giá trị mẫu, không phải khóa: `1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d`.

### Bước 2: Bật truy cập công khai

Ảnh cần mở được bằng trình duyệt, nên bucket phải bật truy cập công khai. Có hai cách:

- **Tên miền riêng (khuyến nghị cho dùng lâu dài):** mở bucket, chọn tab **Settings**, mục **Custom Domains**, bấm **Add**, nhập tên miền, bấm **Continue** rồi **Connect Domain**. Tên miền phải đã là một zone trong cùng tài khoản Cloudflare. Địa chỉ công khai là `https://{tên miền}`.
- **r2.dev:** mở bucket, tab **Settings**, mục **Public Development URL**, bấm **Enable**, gõ `allow` vào hộp xác nhận rồi bấm **Allow**. Địa chỉ công khai có dạng `https://pub-{mã}.r2.dev`. Cloudflare ghi rõ cách này chỉ dùng cho phát triển.

Kho không được bật liệt kê nội dung công khai. Tài liệu Cloudflare cho biết hiện tại truy cập theo từng đối tượng vẫn hoạt động, còn liệt kê thư mục gốc của tên miền thì không.

### Bước 3: Tạo API token chỉ cho một bucket

1. Trong trang **R2 object storage**, mục **Overview**, phần **Account Details**, bấm **Manage** bên cạnh **API Tokens**.
2. Chọn **Create User API token** (gắn với tài khoản cá nhân) hoặc **Create Account API token** (gắn với tài khoản Cloudflare).
3. Ở mục **Permissions**, chọn **Object Read & Write**.
4. Ở phạm vi bucket, chọn đúng bucket của bước 1. Nếu bỏ qua, token có quyền trên mọi bucket của tài khoản.
5. Bấm nút **Create Account API token** hoặc **Create User API token**, tùy loại đã chọn. Cloudflare hiện **Access Key ID** và **Secret Access Key** một lần duy nhất. Hãy lưu cả hai ngay lập tức.

Endpoint của kho có dạng `https://{Account ID}.r2.cloudflarestorage.com`. Ứng dụng tự ghép địa chỉ này từ Account ID.

Khóa này chỉ đọc và ghi đối tượng trong đúng một kho. Nó không có quyền quản lý tài khoản hay kho khác.

### Bước 4: Tạo ba quy tắc vòng đời

Quy tắc vòng đời do bạn tự tạo trên Cloudflare. Ứng dụng không tạo quy tắc và không kiểm tra được quy tắc có tồn tại hay không.

**Nếu không tạo quy tắc, ảnh sẽ không tự hết hạn, dù bạn đã chọn 1 hoặc 7 ngày trong ứng dụng.**

Tạo ba quy tắc:

| Prefix | Số ngày | Ảnh bị xóa sau |
|---|---|---|
| `d1/` | 1 | 1 ngày |
| `d7/` | 7 | 7 ngày |
| `d30/` | 30 | 30 ngày |

Cách tạo:

1. Mở bucket, chọn tab **Settings**, mục **Object Lifecycle Rules**, bấm **Add rule**.
2. Điền tên quy tắc, prefix (ví dụ `d1/`) và số ngày hết hạn. Tên các ô nhập có thể khác trên giao diện hiện tại, nên đối chiếu với tài liệu Cloudflare.
3. Bấm **Save changes**.
4. Lặp lại cho `d7/` và `d30/`. Không tạo quy tắc cho `keep/`.

Sau khi tạo, mở lại mục **Object Lifecycle Rules** và kiểm tra đủ ba quy tắc, prefix đúng từng chữ và số ngày đúng.

Độ trễ xóa: theo tài liệu Cloudflare, đối tượng thường bị xóa trong vòng 24 giờ kể từ thời điểm hết hạn. Ảnh có thể còn trên kho thêm khoảng một ngày sau hạn.

### Bước 5: Nhập vào darkshot

1. Mở Tùy chọn, tab Lưu trữ R2, nhập Account ID, Access Key ID, Secret Access Key, Tên kho và Địa chỉ công khai (ví dụ `https://img.example.com`).
2. Bấm **Kiểm tra kết nối**. Chờ thông báo "Thành công: tải lên được và link mở được."
3. Bấm **OK**.

## Kiểm tra quy tắc xóa

Ba quy tắc chỉ hoạt động nếu đã tạo đúng. Kiểm tra như sau:

1. Trong Tùy chọn, tab Lưu trữ R2, chọn **Hạn của link** là **1 ngày**, bấm **OK**.
2. Chụp một vùng bất kỳ, bấm `Ctrl+D`. Chép link, ví dụ `https://img.example.com/d1/{16 ký tự}.png`. Link phải có `/d1/`.
3. Mở link trong trình duyệt và thấy ảnh.
4. Đợi qua hạn 1 ngày, cộng thêm độ trễ xóa tối đa khoảng 24 giờ. Mở lại link: phải thấy lỗi không tìm thấy.
5. Nếu ảnh vẫn còn sau thời gian này, kiểm tra lại quy tắc `d1/` trong trang Cloudflare.

Sau khi kiểm tra xong, đặt lại **Hạn của link** theo ý bạn.

## Phím PrintScreen

Nếu `PrintScreen` không mở được lớp phủ, một ứng dụng khác đang giữ phím này.

- **Lightshot:** thoát hẳn Lightshot trước khi dùng darkshot. Cách tắt việc Lightshot giữ `PrintScreen` tùy phiên bản; chưa đối chiếu với tài liệu của Lightshot.
- **Snipping Tool của Windows 11:** tắt việc gán `PrintScreen` cho công cụ cắt. Vào **Settings** > **Accessibility** > **Keyboard** rồi tắt mục dùng nút Print Screen để mở công cụ cắt. Tên mục có thể khác theo bản Windows. Tham khảo: https://support.microsoft.com/en-gb/accessibility/windows/make-your-mouse-keyboard-and-other-input-devices-easier-to-use

Nếu không muốn tắt, đổi phím tắt chụp trong Tùy chọn của darkshot (tab Phím tắt) sang tổ hợp khác, ví dụ `Ctrl+Shift+S`. Bỏ tích dòng "Chụp vùng màn hình" cũng trả `PrintScreen` về cho Windows; menu khay vẫn chụp được.

## Cập nhật

- Ứng dụng tự hỏi GitHub xem có bản mới không: một lần sau khoảng 1 phút kể từ khi khởi động, rồi mỗi 24 giờ trong lúc còn chạy. Việc này chạy ở luồng nền, không làm đứng giao diện.
- Mỗi lần hỏi chỉ gửi tên và phiên bản ứng dụng (`User-Agent: darkshot/<phiên bản>`) tới `https://api.github.com/repos/tuanluu-agilityio/darkshot/releases/latest`. Không gửi khóa, ảnh hay cấu hình. Không có mạng, hết thời gian hoặc kết quả lạ thì im lặng, đợi lần sau.
- Có bản mới hơn: menu khay có mục **Có bản mới X.Y.Z** ở trên cùng; mỗi phiên bản được báo một lần bằng thông báo khay. Bấm mục hoặc thông báo thì mở trang release trên GitHub bằng trình duyệt mặc định. Ứng dụng không tự tải hay tự thay file.
- Tắt bằng ô **Tự kiểm tra bản mới** (tab Thông thường của Tùy chọn). Tắt thì không gửi yêu cầu nào và mục menu biến mất. Repo riêng tư thì GitHub trả 404 và ứng dụng coi như không có bản mới.
- Cách cập nhật: tải `darkshot.exe` mới ở trang release, thoát darkshot ở khay (menu **Thoát**), rồi thay file `darkshot.exe` cũ bằng file mới và chạy lại. Cấu hình ở `%APPDATA%\darkshot` được giữ nguyên.

## Bảo mật

- Cấu hình nằm ở `%APPDATA%\darkshot\config.json`.
- **Secret Access Key** được mã hóa bằng DPAPI của Windows, gắn với tài khoản Windows hiện tại, trước khi ghi vào file. Các mục khác ghi dạng văn bản thường. Nếu chép file sang tài khoản Windows khác thì khóa không đọc được, và Tùy chọn sẽ yêu cầu nhập lại.
- Ứng dụng tải ảnh thẳng lên địa chỉ S3 của R2 qua HTTPS, yêu cầu được ký theo AWS Signature V4. Không có máy chủ trung gian.
- Ứng dụng chỉ mở kết nối đến ba địa chỉ: `https://{Account ID}.r2.cloudflarestorage.com`, địa chỉ công khai của kho do bạn nhập, và `https://api.github.com` (chỉ khi ô **Tự kiểm tra bản mới** đang bật, xem mục "Cập nhật"). Tên miền của các địa chỉ này vẫn được hỏi qua máy chủ DNS mà Windows đang dùng, nên máy chủ DNS đó thấy được tên miền.
- Ảnh và link do darkshot chép vào clipboard được đánh dấu bằng các định dạng `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` = 0 và `CanUploadToCloudClipboard` = 0 theo tài liệu của Microsoft, để Windows không đưa chúng vào lịch sử clipboard (`Win+V`) và không đồng bộ sang thiết bị khác. Đây là quy ước của Windows; trình quản lý clipboard của hãng khác có thể không tuân theo.
- Lúc khởi động, darkshot tự loại mình khỏi Báo cáo lỗi của Windows (WER) cho tài khoản hiện tại, để khi ứng dụng sự cố Windows không gửi bản kết xuất bộ nhớ đi. Việc này ghi một giá trị DWORD `darkshot.exe` = 1 vào `HKEY_CURRENT_USER\Software\Microsoft\Windows\Windows Error Reporting\ExcludedApplications`. Muốn gỡ, xóa giá trị đó (ví dụ `reg delete "HKCU\Software\Microsoft\Windows\Windows Error Reporting\ExcludedApplications" /v darkshot.exe /f`). Chính sách WER do quản trị viên đặt cho cả máy vẫn có thể ghi đè. Việc loại trừ có hiệu lực từ sau lần darkshot khởi động đầu tiên, và chưa được thử bằng một sự cố thật.
- Nếu bạn chọn Thoát (hoặc tắt máy, đăng xuất) lúc một lượt tải lên còn chạy, ảnh của lượt đó được lưu PNG vào thư mục lưu trước khi thoát. Có thể ảnh vừa được lưu vừa đã lên kho.
- HTTPS dùng thư viện `rustls` với danh sách chứng chỉ gốc đóng kèm trong file, không dùng bộ chứng chỉ của Windows.
- Cửa sổ Thông tin và Trợ giúp không mở trang web và không gọi mạng. Ứng dụng chỉ mở trình duyệt khi bạn bấm mục hoặc thông báo "Có bản mới", và chỉ tới trang release của repo darkshot trên GitHub.
- Ứng dụng không ghi log ra file. Không gửi thống kê. Việc kiểm tra bản mới chỉ gửi tên và phiên bản ứng dụng (xem mục "Cập nhật") và tắt được.
- Nhận dạng chữ (Ctrl+T) chạy hoàn toàn trên máy bằng OCR của Windows: ảnh và chữ không gửi đi đâu, không ghi ra file, không vào Lịch sử.
- Chụp từng bước: ảnh bước nằm trong `%LOCALAPPDATA%\darkshot\steps\current` cho đến khi bạn xuất ra thư mục bạn chọn. Không gửi đi đâu, không vào Lịch sử. Kết thúc phiên sẽ xóa các ảnh này.
- Ảnh chụp chỉ nằm trong bộ nhớ đến khi bạn chọn Lưu hoặc Tải lên.
- **Ai có link đều xem được ảnh.** Sự riêng tư dựa vào tên ngẫu nhiên khó đoán và hạn của link. Không bật liệt kê nội dung công khai cho kho.
- **Ứng dụng không xóa được ảnh chụp đã tải lên.** (Nó chỉ xóa file thử của nút Kiểm tra kết nối.) Muốn xóa, vào trang Cloudflare R2, mở bucket và xóa đối tượng đó.
- Link "Không hết hạn" nằm trong thư mục `keep/`. Ảnh ở đó vẫn nằm trên kho cho đến khi bạn tự xóa trong trang Cloudflare.

## Phạm vi

Ứng dụng làm việc chính: chụp vùng màn hình, đánh dấu nhanh bằng các công cụ vẽ, chữ, làm mờ, đánh số và chọn để sửa từng hình (mục "Đánh dấu ảnh"), lưu hoặc tải lên và chép link. Có lịch sử ảnh chụp cục bộ (mục "Lịch sử ảnh chụp"); không xóa ảnh đã tải lên từ ứng dụng. Phần đánh dấu chưa có chọn nhiều hình, đổi thứ tự trên dưới hay xoay, nhân bản hình (xem danh sách ở mục "Đánh dấu ảnh").

## Thư viện bên thứ ba

Ảnh JPEG được nén bằng crate `jpeg-encoder`; phần mềm này dựa một phần trên công trình của Independent JPEG Group.
