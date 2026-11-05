# Hướng Dẫn Sử Dụng Chi Tiết — TriHienKun Laucher

Tài liệu này hướng dẫn chi tiết cách vận hành từng phân hệ trong **TriHienKun Laucher**. Toàn bộ giao diện, hộp thoại, thông báo và menu khay hệ thống đều hỗ trợ chuyển đổi tức thì giữa **Tiếng Việt** và **Tiếng Anh**.

---

## 1. Thiết Lập Lần Đầu & Tùy Biến Cá Nhân Hóa
Khi khởi chạy lần đầu tiên, hộp thoại chào mừng cho phép bạn thiết lập nhanh:
- **Ngôn ngữ hiển thị**: Tiếng Việt hoặc Tiếng Anh.
- **Chủ đề giao diện & Tông màu chủ đạo**: Chế độ Tối (`Gaming Dark`) hoặc Sáng (`Light`) kết hợp 6 bảng màu nhấn (`Emerald`, `Cyan`, `Blue`, `Violet`, `Amber`, `Crimson`).
- **Bộ nhớ RAM mặc định**: Áp dụng tự động cho các Profile được tạo mới về sau.

Bạn có thể thay đổi lại bất kỳ thông số nào trong mục **Cài đặt** ở thanh điều hướng bên trái, bao gồm:
- **Hành vi khởi chạy game**: Tự động ẩn cửa sổ Launcher khi vào game (và hiện lại khi thoát game), tự động mở tab Nhật ký trực tiếp khi khởi chạy, hoặc cấp quyền ưu tiên CPU cao (`High CPU Priority`) cho tiến trình Java.
- **Dọn bộ nhớ đệm (`Cache`)**: Xem trực tiếp dung lượng thư mục `cache/` và nhấn **Dọn bộ nhớ đệm** để giải phóng dung lượng đĩa 1 chạm.

---

## 2. Quản Lý Tài Khoản, Đồng Bộ Mojang Cloud & Nhân Vật 3D (`Tài khoản & Skin 3D`)

### 2.1. Đăng Nhập Tài Khoản Microsoft Bản Quyền & Đồng Bộ Skin/Cape
Hỗ trợ 3 phương thức xác thực trực tiếp với Microsoft & Mojang:
1. **Mã Thiết Bị (`microsoft.com/link`)**: Nhấn **Đăng nhập bằng Mã Thiết bị**, sao chép mã 8 ký tự hiển thị trên màn hình và xác nhận trên trình duyệt.
2. **Trình Duyệt (Dán URL Redirect PKCE)**: Nhấn **Đăng nhập Trình duyệt**, đăng nhập tài khoản Microsoft, sau đó dán đường dẫn thanh địa chỉ (`https://login.live.com/oauth20_desktop.srf?code=...`) vào ô xác nhận.
3. **Loopback PKCE với Azure Client ID riêng** *(Nâng cao)*: Điền Application (client) ID riêng để tự động bắt callback OAuth2 trên cổng nội bộ.

> **Đồng bộ Mojang Cloud tự động**: Ngay sau khi đăng nhập Microsoft, Launcher tự động tải và hiển thị đúng **Skin** cùng **Active Cape** bản quyền của bạn trên sân khấu nhân vật 3D mà không cần chọn tệp thủ công. Khi bạn chọn hoặc kéo-thả tệp `.png` mới cho tài khoản Microsoft, Launcher tự động tải Skin đó lên máy chủ Mojang chính thức (`api.minecraftservices.com`)!

### 2.2. Tài Khoản Offline + Phòng Studio 3D (`wgpu`)
- Nhập tên nhân vật (`3–16` ký tự `A-Z`, `0-9`, `_`), chọn dáng tay **Wide (4px)** hoặc **Slim (3px)** và nhấn **Thêm**.
- Ở khung **Phòng Studio Nhân vật 3D** bên phải:
  - Kéo chuột trái xoay 360°, cuộn con lăn thu phóng, hoặc bấm các nút góc máy nhanh (**Chính diện / Áo choàng sau / Góc Hero 3D**).
  - **Kéo-thả trực tiếp tệp `.png`** từ File Explorer vào khung 3D hoặc nhấn **Đổi Skin / Đổi Cape / Đổi Elytra** để áp ngoại hình mới; nhấn **Mặc định** để khôi phục về Skin gốc (đồng thời xóa Skin tùy chỉnh trên Mojang Cloud nếu là tài khoản Microsoft).
  - Khi vào game bằng tài khoản Offline có gắn Skin/Cape/Elytra, Launcher tự động kích hoạt máy chủ Yggdrasil cục bộ để hiển thị đầy đủ ngoại hình ngay trong thế giới Minecraft.

---

## 3. Tạo Profile, Cài Mod, Quản Lý Tệp & Khởi Chạy Đa Phiên Bản (`Trang chủ`)

### 3.1. Tạo Profile Mới hoặc Nhập Gói Có Sẵn
- Nhấn **Tạo Profile mới**: Chọn phiên bản Minecraft (có hỗ trợ Snapshot), chọn 1 trong 5 trình nạp (`Vanilla`, `Fabric`, `Quilt`, `Forge`, `NeoForge`), dung lượng RAM và thư mục lưu trữ.
- Hoặc nhấn **Nhập Modpack (.mrpack / .zip)** để giải nén và cài đặt tự động gói Modpack tải sẵn trên máy.

### 3.2. Chín (9) Thẻ Quản Trị Toàn Diện Cho Từng Profile
- **Cài đặt chung**: Chỉnh tên Profile, RAM cấp phát, đường dẫn Java riêng, tham số JVM, **Độ phân giải cửa sổ tùy chỉnh** (`720p / 900p / 1080p / 1440p` hoặc nhập số pixel bất kỳ) / **Khởi động Toàn màn hình (`Fullscreen`)**, cùng nút **Nhân bản Profile** để tạo bản sao độc lập chứa nguyên bộ Mod & cấu hình.
- **Mods / Shaders / Resource Packs**:
  - Hiển thị ảnh biểu tượng thu nhỏ trực quan, bật/tắt nhanh từng gói bằng hộp kiểm (`.disabled`).
  - Nhấn **Kiểm tra cập nhật** để quét phiên bản mới hàng loạt trên Modrinth & CurseForge, sau đó cập nhật lẻ từng gói hoặc nhấn **Cập nhật tất cả**.
  - Nhấn **Tải thêm trực tuyến** để tìm kiếm có phân trang (**Tải thêm kết quả**), nhận biết gói **Đã cài đặt** và chọn chính xác phiên bản (`Release / Beta / Alpha`) cần cài.
- **Thế giới (Worlds)**: Hiển thị ảnh đại diện thế giới (`saves/<world>/icon.png`), dung lượng, mở nhanh thư mục hoặc xóa thế giới Singleplayer.
- **Tệp tin (Files)**: Duyệt toàn bộ cây thư mục của Profile an toàn (`SafePath`), tải tệp ngoài vào, chỉnh sửa trực tiếp các tệp cấu hình (`.toml`, `.json`, `.json5`, `.cfg`, `.properties`, `.txt`), nén sao lưu `.zip` và giải nén ngay tại chỗ.
- **Server yêu thích**: Lưu địa chỉ máy chủ Multiplayer, kiểm tra độ trễ (`ms`), số người chơi, màu MOTD trực tiếp và nhấn **Vào ngay** (`QuickPlay`).
- **Nhật ký (Logs)**: Xem luồng log trực tiếp đã ảo hóa mượt mà, lọc theo từ khóa hoặc sao chép toàn bộ.
- **Thống kê**: Theo dõi tổng giờ chơi, số phiên mở game, số lần Crash và biểu đồ cột Voxel 3D 14 ngày gần nhất.

---

## 4. Kho Modpack & Nội Dung 3D Đa Nền Tảng (`Kho Modpack`)
- **4 nền tảng tích hợp**: **Modrinth**, **CurseForge**, **FTB Modpacks** và **Technic**; trên Modrinth & CurseForge hỗ trợ chuyển đổi cả 4 loại nội dung (**Modpack, Mod lẻ, Shader, Resource Pack**).
- **Bộ lọc & Hiển thị linh hoạt**: Lọc theo trình nạp Mod, nhánh phiên bản Minecraft, từ khóa và tiêu chí sắp xếp; xem dưới dạng **Lưới thẻ 3D** hoặc **Danh sách**, mở chi tiết Markdown, lọc bản phát hành và phóng to ảnh chụp màn hình bằng **Lightbox**.
- **Trạm Cài Đặt Modpack 3D**: Tùy chỉnh Tên Profile, RAM (`4G / 6G / 8G / 12G`), thư mục riêng hoặc Profile đích; hỗ trợ **Thu gọn xuống thanh trạng thái dưới** và **Hủy cài đặt** bất kỳ lúc nào.

---

## 5. Quản Trị Máy Chủ Cục Bộ Chuyên Dụng (`Máy chủ cục bộ`)

1. Nhấn **Tạo Máy chủ mới**, chọn 1 trong **10 lõi máy chủ** (`Paper`, `Purpur`, `Folia`, `Vanilla`, `Fabric`, `Forge`, `NeoForge`, `Arclight`, `Mohist`, `SpongeVanilla`), phiên bản Minecraft, cổng TCP và mức RAM.
2. Sử dụng 6 thẻ công cụ trực quan:
   - **Bảng điều khiển**: Theo dõi log tô màu trực tiếp, đo `CPU %`, `RAM MB`, `Số người chơi`; nhấn **`↑ / ↓`** để gọi lại các lệnh vừa gửi, **`Tab`** để tự động điền gợi ý lệnh và **`Enter`** để thực thi.
   - **Quản lý Tệp**: Duyệt thư mục, tải tệp lên, sửa tệp cấu hình trực tiếp, nén và giải nén `.zip`.
   - **Plugins / Mods**: Quản lý danh sách `.jar` đã cài trong `plugins/` và `mods/` (bật/tắt, xóa, nhập `.jar` từ máy tính), đồng thời tìm kiếm có phân trang & chọn phiên bản cài đặt trực tiếp từ **Modrinth**, **CurseForge** và **Spiget (SpigotMC)**.
   - **Cấu hình Server**: Chỉnh phần cứng/JVM, sử dụng bảng **Điều khiển Trực quan Nhanh (`server.properties`)** (công tắc `online-mode`, `Whitelist`, `PvP`, `Allow Flight`, `Command Block`, `Hardcore`; hộp chọn `Gamemode`, `Difficulty`; ô kéo số `Max Players`, `View/Sim Distance`, `Spawn Protection`; ô nhập `MOTD`, `World`, `Seed`) hoặc lọc khóa trên bảng thuộc tính thô.
   - **Whitelist & Banlist**: Quản lý danh sách trắng và danh sách cấm đồng bộ tức thì với máy chủ đang chạy.
   - **Mạng & Bore Tunnel**: Sao chép địa chỉ LAN hoặc mở cổng công khai `bore.pub:<port>` không cần mở port modem.

---

## 6. Chia Sẻ Thế Giới Open to LAN (`Chia sẻ LAN`)
1. Mở công cụ **Chia sẻ LAN** ở cột trái và nhấn **Bắt đầu quét LAN**.
2. Vào thế giới Singleplayer trong Minecraft, nhấn `Esc → Open to LAN`.
3. Launcher tự động nhận diện cổng nội bộ xác thực của máy bạn, mở đường hầm `bore.pub` và cung cấp địa chỉ công khai cho bạn bè tham gia.

---

## 7. Quản Lý Java, Tự Sửa Lỗi Crash & Tự Động Cập Nhật
- **Quản lý Java**: Quét toàn bộ Java trên máy và tải thêm `Java 8 / 11 / 17 / 21 / 25` từ **Adoptium Temurin**, **Azul Zulu** hoặc **GraalVM**.
- **Chẩn đoán & Sửa Crash 1 Chạm**: Tự động phân tích nguyên nhân sự cố và cung cấp nút tải ngay Mod thư viện còn thiếu từ Modrinth.
- **Trình Cập Nhật Khởi Động Lại**: Kiểm tra bản phát hành mới trên GitHub, tải và xác minh chữ ký băm `SHA-256`, sau đó nhấn **Áp dụng & Khởi động lại ngay** để tự động thay thế tệp thực thi an toàn (có sao lưu `.bak`) và mở lại Launcher.
