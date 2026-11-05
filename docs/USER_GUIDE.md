# Hướng Dẫn Sử Dụng Chi Tiết — TriHienKun Laucher

Tài liệu này hướng dẫn chi tiết cách vận hành từng phân hệ trong **TriHienKun Laucher**.

---

## 1. Thiết Lập Lần Đầu
Khi khởi chạy lần đầu tiên, hộp thoại chào mừng cho phép bạn thiết lập nhanh:
- **Ngôn ngữ hiển thị**: Tiếng Việt (`🇻🇳`) hoặc Tiếng Anh (`🇬🇧`).
- **Chủ đề giao diện**: Tối (`Gaming Dark`) hoặc Sáng (`Light`).
- **Bộ nhớ RAM mặc định**: Áp dụng tự động cho các Profile được tạo mới về sau.

Bạn có thể thay đổi lại bất kỳ thông số nào trong mục **⚙ Cài đặt** ở thanh điều hướng bên trái.

---

## 2. Quản Lý Tài Khoản & Mô Hình Nhân Vật 3D (`👤 Tài khoản & Skin 3D`)

### 2.1. Đăng Nhập Tài Khoản Microsoft Bản Quyền
Hỗ trợ 3 phương thức xác thực trực tiếp với Microsoft & Mojang:
1. **Mã Thiết Bị (`microsoft.com/link`)**: Nhấn **🔑 Đăng nhập bằng Mã Thiết bị**, sao chép mã 8 ký tự hiển thị trên màn hình và xác nhận trên trình duyệt. Launcher sẽ tự động nhận diện ngay khi bạn hoàn tất.
2. **Trình Duyệt (Dán URL Redirect PKCE)**: Nhấn **🌐 Đăng nhập Trình duyệt**, đăng nhập tài khoản Microsoft, sau đó dán đường dẫn thanh địa chỉ (`https://login.live.com/oauth20_desktop.srf?code=...`) vào ô xác nhận.
3. **Loopback PKCE với Azure Client ID riêng** *(Nâng cao)*: Mở mục nâng cao, điền Application (client) ID của bạn để tự động bắt callback OAuth2 trên cổng nội bộ.

### 2.2. Tài Khoản Offline + Skin / Cape / Elytra Trong Game
- Nhập tên nhân vật (`3–16` ký tự `A-Z`, `0-9`, `_`), chọn dáng tay **Wide (4px)** hoặc **Slim (3px)** và nhấn **➕ Thêm**.
- Ở khung **Mô hình Nhân vật 3D (`wgpu`)** bên phải:
  - Kéo chuột trái để xoay 360°, cuộn con lăn để thu phóng, nhấp đúp chuột để đưa camera về góc chuẩn.
  - Bật/tắt **Lớp giáp ngoài (Outer Layer)**, hoạt ảnh **Đi bộ**, **Tự xoay** và chuyển đổi trang bị lưng giữa **Không / Áo choàng (Cape) / Cánh (Elytra)**.
  - Nhấn **👕 Chọn Skin (.png)**, **🧣 Chọn Cape (.png)** hoặc **🪽 Chọn Elytra (.png)** để nạp tệp ảnh PNG của bạn (hỗ trợ tự động nâng cấp skin `64x32` đời cũ lên chuẩn `64x64`).
  - Khi vào game bằng tài khoản Offline có gắn Skin/Cape, Launcher tự động kích hoạt máy chủ Yggdrasil cục bộ để hiển thị đầy đủ ngoại hình ngay trong thế giới Minecraft.

---

## 3. Tạo Profile, Cài Mod & Khởi Chạy Đa Phiên Bản (`🏠 Trang chủ`)

### 3.1. Tạo Profile Mới hoặc Nhập Gói Có Sẵn
- Nhấn **➕ Tạo Profile mới**:
  - Chọn phiên bản Minecraft (tích **Hiện bản Snapshot** nếu muốn chơi các bản thử nghiệm).
  - Chọn 1 trong 5 trình nạp: `Vanilla`, `Fabric`, `Quilt`, `Forge` hoặc `NeoForge`.
  - Với `Fabric`, tích **Tự động cài bộ Mod tối ưu FPS** để Launcher tự cài `Sodium`, `Lithium`, `FerriteCore`, `EntityCulling` và `ModMenu`.
  - Tùy chỉnh dung lượng RAM và (tùy chọn) nhấn **📂 Chọn thư mục...** nếu muốn lưu Profile ở ổ đĩa riêng.
- Hoặc nhấn **📥 Nhập Modpack (.mrpack / .zip)** để giải nén và cài đặt tự động gói Modpack tải sẵn trên máy.

### 3.2. Quản Lý Nội Dung Trong Từng Profile
Mỗi Profile cung cấp 8 thẻ quản trị chuyên biệt:
- **Cài đặt chung**: Chỉnh tên Profile, RAM cấp phát, đường dẫn Java riêng và tham số JVM bổ sung.
- **Mods / Shaders / Resource Packs**:
  - Bật/tắt nhanh từng tệp bằng hộp kiểm (tự động đổi đuôi `.disabled` an toàn).
  - Tự động nhận diện tên & phiên bản các tệp `.jar` chép tay bằng mã băm `SHA-1` trên Modrinth.
  - Nhấn **🌐 Tìm & Cài trực tuyến** để tìm và cài đặt trực tiếp từ **Modrinth** hoặc **CurseForge** (tự động tải kèm toàn bộ Mod thư viện phụ thuộc bắt buộc).
- **Thế giới (Worlds)**: Xem dung lượng, mở nhanh thư mục `saves/<world>` hoặc xóa thế giới Singleplayer.
- **Server yêu thích**: Lưu địa chỉ máy chủ Multiplayer, kiểm tra độ trễ (`ms`), số người chơi, màu sắc MOTD theo thời gian thực và nhấn **▶ Vào ngay** để khởi chạy Minecraft vào thẳng máy chủ đó.
- **Nhật ký (Logs)**: Xem luồng log trực tiếp của game đã ảo hóa mượt mà, lọc theo từ khóa hoặc sao chép toàn bộ.
- **Thống kê**: Theo dõi tổng giờ chơi, số phiên mở game, số lần sự cố và biểu đồ cột thời lượng chơi 14 ngày gần nhất.

---

## 4. Kho Modpack Trực Tuyến 4 Nền Tảng (`📦 Kho Modpack`)
- Chuyển đổi tức thì giữa **Modrinth**, **CurseForge**, **FTB Modpacks** và **Technic**.
- Lọc theo từ khóa, phiên bản Minecraft và tiêu chí sắp xếp (*Lượt tải cao nhất, Độ liên quan, Mới cập nhật, Mới phát hành*).
- Nhấn **Xem & Cài đặt ➡** để đọc mô tả chi tiết định dạng Markdown, chọn phiên bản mong muốn và nhấn **⬇ Cài đặt & Tạo Profile mới**.
- *(Lưu ý với CurseForge)*: Một số gói trên CurseForge yêu cầu khóa API cá nhân — bạn có thể điền khóa `$2a$10$...` tại **⚙ Cài đặt → Khoá API CurseForge**.

---

## 5. Quản Trị Máy Chủ Cục Bộ Chuyên Dụng (`🖥 Máy chủ cục bộ`)

1. Nhấn **➕ Tạo Máy chủ mới**, chọn 1 trong **10 lõi máy chủ** (`Paper`, `Purpur`, `Folia`, `Vanilla`, `Fabric`, `Forge`, `NeoForge`, `Arclight`, `Mohist`, `SpongeVanilla`), chọn phiên bản Minecraft, cổng TCP, mức RAM tối thiểu/tối đa và thư mục lưu trữ.
2. Khi khởi động lần đầu, Launcher tự động xác nhận `eula.txt`, chuẩn bị bản Java tương thích và khởi chạy máy chủ ngầm không hiện cửa sổ CMD đen.
3. Sử dụng 6 thẻ công cụ trực quan:
   - **🖥 Bảng điều khiển**: Theo dõi log có tô màu mức độ cảnh báo, đo `CPU %`, `RAM MB`, `Số người chơi` trực tiếp; gõ lệnh và nhấn phím **`Tab`** để tự động điền gợi ý lệnh quản trị.
   - **📁 Quản lý Tệp**: Duyệt cây thư mục máy chủ, tải tệp từ bên ngoài lên, sửa trực tiếp các tệp cấu hình (`.yml`, `.properties`, `.json`, `.toml`, `.txt`), nén sao lưu `.zip` và giải nén ngay tại chỗ.
   - **🧩 Plugins / Mods**: Tìm và cài đặt trực tiếp Plugin/Mod từ **Modrinth** và **Spiget (SpigotMC)** khớp với lõi máy chủ đang dùng.
   - **⚙ Cấu hình Server**: Chỉnh sửa phần cứng (Tên, Port, Min/Max RAM, Java Path, JVM Flags) và bảng khóa-giá trị `server.properties` (tự động giữ nguyên các dòng chú thích `#`).
   - **🛡 Whitelist & Banlist**: Thêm/gỡ người chơi trong danh sách trắng hoặc danh sách cấm (đồng bộ lệnh trực tiếp vào máy chủ nếu đang trực tuyến).
   - **🌐 Mạng & Bore Tunnel**: Sao chép địa chỉ LAN hoặc nhấn **🌐 Mở Bore Tunnel** để nhận ngay địa chỉ công khai `bore.pub:<port>` cho bạn bè ở khác mạng Internet tham gia mà không cần mở port modem.

---

## 6. Chia Sẻ Thế Giới Open to LAN (`🌐 Chia sẻ LAN`)
1. Mở công cụ **🌐 Chia sẻ LAN** ở cột trái và nhấn **📡 Bắt đầu quét LAN**.
2. Vào thế giới Singleplayer trong Minecraft, nhấn `Esc → Open to LAN`.
3. Launcher sẽ tự động bắt gói phát sóng nội bộ của máy bạn, mở đường hầm `bore.pub` và hiển thị địa chỉ công khai để bạn sao chép gửi cho bạn bè.

---

## 7. Quản Lý Java, Cá Nhân Hóa Giao Diện & Tự Sửa Lỗi Crash
- **☕ Quản lý Java**: Quét toàn bộ các bản Java trên máy và tải thêm `Java 8 / 11 / 17 / 21 / 25` từ **Adoptium Temurin**, **Azul Zulu** hoặc **GraalVM**.
- **🖼 Ảnh nền & Nhạc nền riêng**: Trong **⚙ Cài đặt**, bạn có thể chọn ảnh nền cá nhân (`PNG / JPG / WEBP`), điều chỉnh số luồng tải song song (`2–32` luồng), bật nhạc nền Ambient tổng hợp hoặc chọn tệp nhạc riêng (`MP3 / OGG / WAV`).
- **⚠ Chẩn đoán & Sửa Crash 1 Chạm**: Khi game gặp sự cố thoát bất thường, cửa sổ chẩn đoán tự động hiển thị nguyên nhân gốc cùng nút **🔧 Tự động tải & sửa** để tải ngay Mod thư viện còn thiếu từ Modrinth hoặc mở trình nâng cấp Java phù hợp.
