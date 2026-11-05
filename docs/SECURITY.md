# Mô Hình Bảo Mật & Kiểm Soát An Toàn — TriHienKun Laucher

Tài liệu này trình bày các cơ chế bảo vệ cấp mã nguồn, mạng, tệp tin và xác thực được áp dụng xuyên suốt trong **TriHienKun Laucher**, cùng bảng đối chiếu khắc phục triệt để các rủi ro kiến trúc từ bản tham chiếu Electron cũ.

---

## 1. Các Lớp Phòng Thủ Cốt Lõi

### 1.1. An Toàn Bộ Nhớ Tuyệt Đối (`#![deny(unsafe_code)]`)
Toàn bộ 9 crates (`thk-core`, `thk-auth`, `thk-java`, `thk-mods`, `thk-launcher`, `thk-network`, `thk-server`, `thk-renderer3d`, `thk-app`) đều đặt chỉ thị `#![deny(unsafe_code)]` ở đầu tệp gốc, ngăn chặn hoàn toàn lỗi tràn bộ đệm, con trỏ treo và tranh chấp vùng nhớ không an toàn.

### 1.2. Chống Thoát Thư Mục & Chống Zip-Slip (`SafePath` & `archive.rs`)
- **`SafePath::resolve_child(base, relative)`**:
  - Từ chối ngay lập tức mọi đường dẫn chứa ký tự rỗng `\0`, thành phần `Component::ParentDir` (`..`), `Component::RootDir` (`/`, `\`) hoặc tiền tố ổ đĩa `Component::Prefix` (`C:`).
  - Thực hiện chuẩn hóa đường dẫn thực (`canonicalize`) trên tổ tiên tồn tại gần nhất để đảm bảo đích đến cuối cùng luôn nằm bên trong `base` ngay cả khi có liên kết tượng trưng (`symlink` / `junction`).
- **`SafePath::validate_single_name(name)`**: Ràng buộc tên tệp đơn lẻ không chứa `/`, `\`, `:`, `\0`, `.` hoặc `..` — áp dụng bắt buộc khi đổi tên Mod (`.disabled`), tải tệp từ API bên ngoài hoặc giải nén.
- **Giải nén chống Zip-Slip (`extract_zip_safe` & `extract_tar_gz_safe`)**: Từng mục bên trong gói `.zip`, `.mrpack`, `.jar` hoặc `.tar.gz` đều đi qua `SafePath::resolve_child` trước khi tạo thư mục hoặc ghi tệp xuống đĩa.

### 1.3. Làm Sạch Đầu Vào Bảng Điều Khiển Máy Chủ (`sanitize_console_command`)
- Trong `crates/thk-server/src/manager.rs`, mọi lệnh gửi vào `stdin` của tiến trình máy chủ Minecraft đều đi qua `sanitize_console_command`: loại bỏ triệt để ký tự xuống dòng `\r`, `\n` và toàn bộ byte điều khiển ASCII (`< 0x20` ngoại trừ khoảng trắng), chặn đứng tấn công chèn đa lệnh (Command Smuggling) khi dán văn bản từ nguồn bên ngoài.
- Khi ghi đè khóa trong `server.properties` (`apply_properties_patch_str`), cả tên khóa và giá trị đều bị lọc sạch `\r` và `\n`, ngăn ngừa chèn dòng cấu hình giả mạo.

### 1.4. Xác Thực Nguồn Gói Quảng Bá LAN (`thk-network/src/lan_tunnel.rs`)
Minecraft quảng bá thế giới `Open to LAN` qua nhóm UDP Multicast `224.0.2.60:4445` không có xác thực. Để ngăn kẻ tấn công trong cùng mạng Wi-Fi công cộng phát gói `[MOTD]...[/MOTD][AD]<port>[/AD]` giả mạo khiến Launcher tự động mở đường hầm công khai vào cổng nhạy cảm:
1. **Kiểm tra quyền sở hữu địa chỉ IP (`is_local_interface_ipv4`)**: Gói UDP chỉ được chấp nhận nếu địa chỉ IP nguồn (`sender_ip`) thực sự thuộc về một giao diện mạng trên chính máy tính đang chạy (kiểm chứng bằng phép thử gắn kết `UdpSocket::bind((sender_ip, 0))`). Mọi gói tin từ máy khác bị loại bỏ và ghi nhận vào bộ đếm cảnh báo `ignored_foreign_packets`.
2. **Kiểm tra cổng lắng nghe cục bộ (`is_local_tcp_port_reachable`)**: Trước khi khởi tạo đường hầm `bore`, hệ thống xác minh cổng TCP đó thực sự đang mở trên `127.0.0.1`.

### 1.5. Cô Lập Khóa OAuth2 & Không Phụ Thuộc Máy Chủ Trung Gian
- Hỗ trợ đầy đủ chuẩn **OAuth2 PKCE (`S256`)** và **Device Code Flow** kết nối trực tiếp tới các điểm cuối chính thức của Microsoft Live, Xbox Live, XSTS và Minecraft Services — không gửi bất kỳ mã xác thực nào qua máy chủ bên thứ ba.
- Máy chủ Yggdrasil phục vụ Skin/Cape Offline chỉ lắng nghe trên giao diện nội bộ `127.0.0.1:0` (không mở ra mạng LAN/Internet) và tự động hủy ngay khi tiến trình Minecraft đóng lại.
- Toàn bộ token nhạy cảm được lưu trong **OS Keyring** thay vì tệp văn bản thuần trên ổ cứng.

### 1.6. Xác Thực Mã Băm Đa Tầng Cho Mọi Tệp Tải Về & Bản Cập Nhật
- `DownloadManager` tải dữ liệu vào tệp tạm ẩn `.part`, tính toán mã băm trực tiếp trên luồng dữ liệu tải về (`SHA-1`, `SHA-256`, `SHA-512`) và xóa ngay tệp `.part` nếu mã băm thực tế sai lệch dù chỉ 1 bit trước khi đổi tên nguyên tử sang tệp đích.
- Bộ cập nhật ứng dụng (`crates/thk-app/src/updater.rs`) **bắt buộc** đối chiếu mã băm `SHA-256` từ `SHA256SUMS.txt` hoặc trường `digest` chính thức trên GitHub Release; từ chối tải nếu bản phát hành không cung cấp mã băm xác thực.

---

## 2. Bảng Đối Chiếu Khắc Phục So Với Bản Tham Chiếu Cũ (`VoxelXClient`)

| Hạng Mục Rủi Ro Ở Kiến Trúc Cũ | Giải Pháp Đã Triển Khai Trong `TriHienKun Laucher` |
| :--- | :--- |
| Lưu token tài khoản dạng văn bản thuần trong tệp JSON trên đĩa. | Lưu trữ mã hóa trong **OS Keyring** (`Windows Credential Manager` / `Keychain` / `Secret Service`) qua `TokenStore`. |
| Phụ thuộc backend đóng kín từ xa để trung chuyển đăng nhập Microsoft. | Giao tiếp OAuth2 PKCE `S256` & Device Code trực tiếp với Microsoft / Xbox Live / Minecraft Services. |
| Nối chuỗi đường dẫn trực tiếp khi đọc/xóa/giải nén tệp máy chủ và Modpack. | Kiểm chứng bắt buộc qua `SafePath::resolve_child`, `validate_single_name` và `extract_zip_safe`. |
| Chấp nhận mọi gói UDP Multicast `224.0.2.60:4445` từ bất kỳ máy nào trong mạng LAN. | Xác minh địa chỉ IP nguồn thuộc card mạng của chính máy cục bộ (`is_local_interface_ipv4`) trước khi mở Bore Tunnel. |
| Tải nhị phân `bore.exe` ngoài luồng không kiểm tra chữ ký khi chia sẻ LAN. | Nhúng trực tiếp thư viện thuần Rust `bore-cli = "0.6.0"` ngay trong tệp thực thi duy nhất. |
| Sao chép tệp tải lên máy chủ bằng cách đọc toàn bộ vào bộ nhớ RAM (`readFileSync`). | Sao chép luồng bất đồng bộ (`tokio::fs::copy` trong `import_external_file_streaming`) không gây tràn RAM với tệp thế giới nhiều GB. |
| Ghi đè trạng thái cấu hình và thời gian chơi vào một tệp JSON đơn lẻ gây mất dữ liệu khi chạy nhiều cửa sổ. | Tách cột cập nhật nguyên tử trên **SQLite WAL** kết hợp khóa tiến trình `InstanceKey { profile_id, account_id }`. |
