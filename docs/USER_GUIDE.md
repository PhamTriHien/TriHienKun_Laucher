# Hướng Dẫn Sử Dụng Chi Tiết — TriHienKun Laucher

Tài liệu này hướng dẫn chi tiết cách vận hành từng phân hệ trong **TriHienKun Laucher**. Toàn bộ giao diện, hộp thoại, thông báo và menu khay hệ thống đều hỗ trợ chuyển đổi tức thì giữa **Tiếng Việt** và **Tiếng Anh**.

---

## 1. Thiết Lập Lần Đầu & Tùy Biến Cá Nhân Hóa
Khi khởi chạy lần đầu tiên, hộp thoại chào mừng cho phép bạn thiết lập nhanh:
- **Ngôn ngữ hiển thị**: Tiếng Việt hoặc Tiếng Anh.
- **Chủ đề Engine Game Tươi Sáng, Đa cửa sổ Desktop, Bố cục Dọc/Ngang, Kéo giãn khung cửa sổ & Cỡ chữ tự do**: Mặc định chủ đề **Engine Game Sáng (`Light`)** tươi sáng hoặc tùy chọn **Tối (`Cobalt Dusk`)**, kết hợp 6 bảng màu nhấn (`Emerald`, `Cyan`, `Blue`, `Violet`, `Amber`, `Crimson`), bộ nút bấm & biểu tượng 3D tạo bởi AI có hiệu ứng nghiêng phối cảnh thời gian thực, thanh kéo kết hợp **ô nhập/kéo số phần trăm trực tiếp (`50.0% – 200.0%`, độ chính xác `0.1%`, mặc định `100%`)** đồng bộ sắc nét toàn bộ chữ và hộp thoại, hỗ trợ **mở Đa cửa sổ độc lập trên Desktop** (**Cửa sổ Đa nhiệm** ở thanh bên trái để vừa quản lý Máy chủ vừa xem Kho Modpack/Profile song song, **Tách cửa sổ Nhật ký** trong Profile → Nhật ký và **Tách cửa sổ Console** trong Máy chủ → Bảng điều khiển), tự động chuyển sang **bố cục Màn hình Dọc (Thanh công cụ đỉnh + Thanh Dock 4 tab dưới cùng)** khi xoay dọc điện thoại hoặc thu hẹp cửa sổ, đồng thời cho phép **nhấn giữ chuột kéo giãn trực tiếp** ranh giới các thanh bên cũng như di chuyển và kéo giãn 4 cạnh / 4 góc của mọi cửa sổ hộp thoại.
- **Bộ nhớ RAM mặc định**: Áp dụng tự động cho các Profile được tạo mới về sau.

Bạn có thể thay đổi lại bất kỳ thông số nào trong mục **Cài đặt** ở thanh điều hướng bên trái, bao gồm:
- **Hành vi khởi chạy game & Chế độ Siêu tiết kiệm RAM**: Tự động ẩn cửa sổ Launcher khi vào game (và hiện lại khi thoát game), tự động mở tab Nhật ký trực tiếp khi khởi chạy, cấp quyền ưu tiên CPU cao cho tiến trình Java, hoặc bật **Chế độ Siêu tiết kiệm RAM cho Launcher** để dừng hiệu ứng nền khi rảnh, thu nhỏ/giải phóng bộ nhớ kết xuất 3D và tự động thu gọn bộ nhớ đệm ảnh/log (đặc biệt hữu ích khi vừa chạy Minecraft nặng vừa mở máy chủ cục bộ).
- **Dọn bộ nhớ đệm (`Cache`) & Dữ liệu phục hồi (`Recovery Artifacts`)**: Xem trực tiếp dung lượng thư mục `cache/`, xác nhận 2 bước để dọn bộ nhớ đệm trên luồng nền (tự động xóa trạng thái tệp cập nhật cũ nếu đã tải trước đó), đồng thời kiểm tra danh sách các thư mục/tệp phục hồi (`.thk-pack-*`, `.thk-install-*`, `.backup`, `.failed`, `.update-lock`) kèm nút **Mở vị trí** và xác nhận dọn dẹp an toàn khi không có tác vụ nào đang chạy.
- **Khóa API CurseForge tùy chọn**: Khi để trống, ứng dụng tự động dùng điểm cuối proxy công khai `api.curse.tools`; nếu bạn điền khóa riêng không hợp lệ (`HTTP 401/403`), ứng dụng sẽ hiển thị cảnh báo rõ ràng để kiểm tra lại khóa.
- **Bảo vệ cấu hình đã lưu**: Nếu dữ liệu `app_settings` trong cơ sở dữ liệu bị hỏng định dạng JSON, Launcher hiển thị hộp thoại thông báo lỗi song ngữ và dừng khởi chạy thay vì tự động xóa cấu hình cũ về mặc định; nếu việc lưu cài đặt gặp lỗi I/O/SQLite trong lúc chạy, thông báo Toast sẽ cảnh báo rằng thay đổi chỉ có hiệu lực trong phiên hiện tại.

---

## 2. Quản Lý Tài Khoản, Đồng Bộ Mojang Cloud & Nhân Vật 3D (`Tài khoản & Skin 3D`)

### 2.1. Đăng Nhập Tài Khoản Microsoft Bản Quyền & Đồng Bộ Skin/Cape
Hỗ trợ 3 phương thức xác thực trực tiếp với Microsoft & Mojang:
1. **Mã Thiết Bị (`microsoft.com/link`)**: Nhấn **Đăng nhập bằng Mã Thiết bị**, sao chép mã 8 ký tự hiển thị trên màn hình và xác nhận trên trình duyệt.
2. **Trình Duyệt (Dán URL Redirect PKCE)**: Nhấn **Đăng nhập Trình duyệt**, đăng nhập tài khoản Microsoft, sau đó dán đường dẫn thanh địa chỉ (`https://login.live.com/oauth20_desktop.srf?code=...`) vào ô xác nhận.
3. **Loopback PKCE với Azure Client ID riêng** *(Nâng cao)*: Mở mục **Nâng cao: Đăng nhập Loopback PKCE với Azure Client ID riêng**, điền Application (client) ID, rồi nhấn **Mở cổng Loopback PKCE**. Nhấn lại tiêu đề để thu gọn; ô Client ID và nút bắt đầu chỉ hiện khi mục này mở. Công cụ trợ năng có thể dùng hành động Expand/Collapse. Client ID và redirect URI phải phù hợp cấu hình ứng dụng Microsoft của bạn.

> Không chia sẻ URL redirect chứa `code`, token hoặc ảnh chụp có mã đăng nhập. Việc mở/đóng mục PKCE đã được kiểm tra riêng; đăng nhập Microsoft tới Minecraft session vẫn cần người dùng tự xác thực và chưa được xác minh đầu-cuối trong đợt kiểm thử này.

> **Đồng bộ Mojang Cloud tự động**: Ngay sau khi đăng nhập Microsoft, Launcher tự động tải và hiển thị đúng **Skin** cùng **Active Cape** bản quyền của bạn trên sân khấu nhân vật 3D mà không cần chọn tệp thủ công. Khi bạn chọn hoặc kéo-thả tệp `.png` mới cho tài khoản Microsoft, Launcher tự động tải Skin đó lên máy chủ Mojang chính thức (`api.minecraftservices.com`)!

### 2.2. Tài Khoản Offline + Phòng Studio Nhân Vật 3D
- Nhập tên nhân vật (`3–16` ký tự `A-Z`, `0-9`, `_`), chọn dáng tay **Wide (4px)** hoặc **Slim (3px)** và nhấn **Thêm**.
- Ở khung **Mô hình Nhân vật 3D** bên phải:
  - Mặc định hiển thị đúng **Skin Steve `64×64` nguyên bản** của Minecraft (tự trích xuất từ phiên bản game đã cài trên máy, không tự gắn áo choàng phụ khi tài khoản chưa chọn Cape/Elytra).
  - Kéo chuột trái xoay 360°, cuộn con lăn thu phóng, hoặc bấm các nút góc máy nhanh (**Chính diện / Áo choàng sau / Góc Hero 3D**).
  - **Kéo-thả trực tiếp tệp `.png`** từ File Explorer vào khung 3D hoặc nhấn **Đổi Skin / Đổi Cape / Đổi Elytra** để áp ngoại hình mới; nhấn **Mặc định** để khôi phục về Skin gốc (đồng thời xóa Skin tùy chỉnh trên Mojang Cloud nếu là tài khoản Microsoft).
  - Khi vào game bằng tài khoản Offline có gắn Skin/Cape/Elytra, Launcher tự động kích hoạt máy chủ Yggdrasil cục bộ để hiển thị đầy đủ ngoại hình ngay trong thế giới Minecraft.

---

## 3. Tạo Profile, Cài Mod, Quản Lý Tệp & Khởi Chạy Đa Phiên Bản (`Trang chủ`)

### 3.1. Tạo Profile Mới hoặc Nhập Gói Có Sẵn
- Nhấn **Tạo Profile mới**: Chọn phiên bản Minecraft (có hỗ trợ Snapshot), chọn Vanilla hoặc một trong 4 mod loader (`Fabric`, `Quilt`, `Forge`, `NeoForge`), dung lượng RAM và thư mục lưu trữ.
- Hoặc nhấn **Nhập Modpack (.mrpack / .zip)** để mở **Trạm Nhập Modpack từ đĩa**: tự động nhận diện tên gói, phiên bản Minecraft, Mod Loader và số lượng tệp trong manifest, cho phép tùy chỉnh Tên Profile, mức RAM và thư mục đích (phải là thư mục mới hoặc rỗng, không thuộc Profile khác), đồng thời hỗ trợ **Thu gọn xuống thanh trạng thái dưới** hoặc **Hủy nhập** giữa chừng.
- Mọi thao tác xóa **Profile**, **Thế giới Singleplayer** hoặc **Tệp tin** đều hiển thị hộp thoại xác nhận trước khi thực thi (với tùy chọn xóa kèm toàn bộ thư mục dữ liệu trên đĩa, mặc định tắt đối với thư mục tùy chỉnh bên ngoài).

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
- **Trạm Cài Đặt Modpack 3D**: Cho phép chọn **Đích cài đặt** là **Profile chơi game** hoặc **Tạo Máy chủ** trực tiếp từ Cloud; tùy chỉnh Tên Profile / Tên Máy chủ, cổng TCP, RAM (`4G / 6G / 8G / 12G`), thư mục riêng và xác nhận Minecraft EULA (khi tạo máy chủ); hỗ trợ **Thu gọn xuống thanh trạng thái dưới** và **Hủy cài đặt**. Thu gọn chỉ ẩn bảng tiến trình, không hủy tác vụ.
- **Chọn đích cài**: Thư mục phải chưa tồn tại hoặc rỗng, không phải symlink và không thuộc Profile khác. Nếu bị từ chối, chọn một thư mục mới; không xóa dữ liệu cũ chỉ để vượt kiểm tra. Pack được chuẩn bị trong `.thk-pack-<UUID>/payload` cạnh đích, sau đó mới sao chép và lưu Profile vào SQLite.
- **Hủy là yêu cầu có phối hợp & Quản lý đa tiến trình**: Cả Trạm Cài Đặt trực tuyến, Trạm Nhập Modpack từ đĩa lẫn quá trình chuẩn bị khởi chạy game đều nối `CancellationToken` và hiển thị trên thanh trạng thái dưới cùng (kèm huy hiệu `+N` khi có nhiều tác vụ chạy song song và nút **Hủy** trực tiếp). Giải nén kiểm tra token giữa các tệp và từng khối đọc tối đa 64 KiB; vẫn phải chờ thao tác I/O đang chạy và worker đóng tệp, không bảo đảm dừng tức thì khi ổ đĩa bị chặn hoặc đang sao chép commit. Đóng cưỡng bức tiến trình không tương đương với hủy bằng nút.
- **Không bỏ qua mod bắt buộc**: Nếu Modrinth, CurseForge, FTB hoặc Technic không tải/lấy được URL hoặc trả tên tệp không an toàn cho một mục bắt buộc, lượt cài dừng và báo lỗi rõ ràng; không tạo Profile báo thành công khi thiếu mod bắt buộc. Sau khi commit, toàn bộ danh sách mod trong gói được lưu nguyên tử vào bảng `installed_content` để kiểm tra cập nhật ngay lập tức (kể cả nhận diện vân tay Murmur2 CurseForge).
- **Nếu cài lỗi**: Bộ cài cố hoàn tác các tệp/thư mục do lượt cài tạo. Nếu rollback không hoàn tất, thông báo trả đường dẫn staging được giữ cùng `recovery.json`. Giữ nguyên thư mục này, payload và dữ liệu đích; đối chiếu manifest với Profile trong SQLite trước khi phục hồi thủ công. Không có tự phục hồi sau mất điện/crash hoặc bảo đảm nguyên tử giữa filesystem và SQLite; không chạy hai launcher/công cụ khác cùng sửa đích cài.

> Live Modrinth đã kiểm tải, hash và lưu Profile của một pack cố định; chưa kiểm khởi chạy pack đó. Không suy ra mọi pack từ cả bốn nguồn đều tương thích. Chi tiết ở [TESTING.md](TESTING.md).

---

## 5. Quản Trị Máy Chủ Cục Bộ Chuyên Dụng (`Máy chủ cục bộ`)

1. Nhấn **Tạo Máy chủ mới** và chọn 1 trong **4 chế độ khởi tạo**:
   - **Lõi tiêu chuẩn**: Chọn 1 trong **10 lõi máy chủ** (`Paper`, `Purpur`, `Folia`, `Vanilla`, `Fabric`, `Forge`, `NeoForge`, `Arclight`, `Mohist`, `SpongeVanilla`), phiên bản Minecraft, cổng TCP và mức RAM. Paper/Folia dùng Fill v3 và chỉ cài build `STABLE` mới nhất của đúng phiên bản (Folia 1.21.4 chưa có build `STABLE` tại lần kiểm).
   - **Từ Cloud (`Modrinth / CurseForge / FTB / Technic`)**: Tìm kiếm trực tiếp gói Modpack trực tuyến kèm hiển thị ảnh thu nhỏ từng gói, chọn đúng **phiên bản chuẩn của Modpack** (ví dụ `V8E`, `v0.3.2`, tự động ưu tiên gói `Server Pack` chính thức trên CurseForge nếu nhà phát triển cung cấp), lọc bỏ các mod/thư mục chỉ dành cho Client và tự động cài đặt lõi máy chủ tương ứng. Bạn cũng có thể tạo máy chủ trực tiếp ngay từ mục **Kho Modpack → Cài đặt → Tạo Máy chủ**.
   - **Từ tệp nén (`.mrpack` / `.zip`)**: Chọn tệp `.mrpack` Modrinth, `.zip` CurseForge hoặc gói Dedicated Server `.zip` trên máy tính; Launcher tự phân tích phiên bản Minecraft & Mod Loader, lọc bỏ các mod/thư mục chỉ dành cho Client và tự động cài đặt lõi máy chủ tương ứng.
   - **Từ Profile có sẵn**: Chọn một Profile đã cài đặt trong Launcher để tự động sao chép `mods/`, `config/`, `defaultconfigs/`, `kubejs/`, `scripts/`, `datapacks/` tương thích sang thư mục máy chủ mới và cài lõi `Fabric` / `Forge` / `NeoForge` / `Vanilla` khớp phiên bản.
   - **Chấp thuận Minecraft EULA tường minh**: Hộp thoại Tạo Máy chủ cung cấp liên kết đọc [Minecraft EULA](https://www.minecraft.net/eula) cùng ô đánh dấu đồng ý (mặc định bỏ chọn). Nếu nhấn **Start/Restart** trên máy chủ chưa có `eula=true`, Launcher mở hộp thoại xác nhận điều khoản ngay trong ứng dụng trước khi ghi `eula=true` và khởi chạy JVM.
   - Nếu cấu hình đường dẫn Java tùy chỉnh cho máy chủ, Launcher kiểm tra phiên bản chính (`major_version`) của tệp Java đó trước khi chạy và từ chối sớm nếu thấp hơn mức Minecraft yêu cầu (`Java 8 / 16 / 17 / 21 / 25`).
2. Sử dụng 6 thẻ công cụ trực quan:
   - **Bảng điều khiển**: Theo dõi log tô màu trực tiếp, hiển thị tên vi xử lý / SoC, đo chính xác `CPU %`, `Số lõi đang sử dụng / Tổng số lõi` (đa nền tảng Windows, Linux, macOS & Android), `RAM MB`, **`TPS / MSPT` thực tế**, **`Ping` nội bộ & `Internet` công khai**, `Số người chơi`; nhấn **`↑ / ↓`** để gọi lại các lệnh vừa gửi, **`Tab`** để tự động điền gợi ý lệnh và **`Enter`** để thực thi.
   - **Quản lý Tệp**: Duyệt thư mục, tải tệp lên, sửa tệp cấu hình trực tiếp, nén và giải nén `.zip` trên luồng nền.
   - **Plugins / Mods**: Quản lý danh sách `.jar` đã cài trong `plugins/` và `mods/` (bật/tắt, xóa, nhập `.jar` từ máy tính), đồng thời tìm kiếm có phân trang, tự động phân giải phụ thuộc bắt buộc & cài đặt an toàn qua vùng staging từ **Modrinth**, **CurseForge** và **Spiget (SpigotMC)**.
   - **Cấu hình Server**: Xem thông tin **CPU hệ thống** (tên đầy đủ vi xử lý / cụm nhân ARM big.LITTLE, số nhân vật lý / luồng xử lý và số lõi được hệ điều hành cấp phép), chỉnh phần cứng/JVM, chọn **Tệp khởi chạy (`JAR` hoặc `@libraries/.../win_args.txt` / `unix_args.txt`)** cho máy chủ tùy chọn hoặc Forge/NeoForge hiện đại, sử dụng bảng **Điều khiển Trực quan Nhanh (`server.properties`)** (công tắc `online-mode`, `Whitelist`, `PvP`, `Allow Flight`, `Command Block`, `Hardcore`; hộp chọn `Gamemode`, `Difficulty`; ô kéo số `Max Players`, `View/Sim Distance`, `Spawn Protection`; ô nhập `MOTD`, `World`, `Seed`) hoặc lọc khóa trên bảng thuộc tính thô.
   - **Người chơi & Quyền**: Thiết lập trực tiếp **Quy tắc Người chơi** (`Chế độ chơi mặc định`, `Tối đa người chơi`, `Treo máy tự kick`, `Bảo vệ khu Spawn`, `Mức quyền OP mặc định`, `PvP`, `Cho phép bay`, `Ép chế độ chơi`, `Whitelist`, `Đuổi ngay người ngoài Whitelist`, `Ẩn danh sách người chơi`), theo dõi **Người chơi Trực tuyến & Đã tham gia** (thao tác 1 chạm `Sinh tồn / Sáng tạo / Khán giả / Xóa túi đồ / Kick` khi đang online, cấp/gỡ `OP`, `Whitelist`, `Cấm`, hoặc **Đặt lại dữ liệu `.dat`** 2 bước khi offline), cùng 4 danh sách **Quản trị viên OP (`Cấp 1–4`, `Vượt giới hạn slot`)**, **Danh sách Trắng**, **Cấm Tài khoản** và **Cấm Địa chỉ IP (`IPv4 / IPv6`)**.
   - **Mạng & Triển khai**: Mở cổng trực tiếp theo mạng đang kết nối (**Wi-Fi / Ethernet / Điểm phát sóng / VPN**) với cổng tùy chọn (`25565` hoặc cổng bất kỳ), tự động phát sóng vào mục *Chơi mạng (Multiplayer)* trong LAN, cung cấp địa chỉ **Online IPv6 trực tiếp (`[<ipv6>]:<port>`)** không qua máy chủ trung gian nước ngoài, hỗ trợ **mở cổng tự động trên Modem** (báo trung thực trạng thái mở thành công, lớp IP chung nhà mạng hay Modem tắt tự động mở cổng), dự phòng đường hầm `bore.pub:<port>` kèm đo **Ping thực tế & số kết nối đang mở**, hoặc cấu hình **Triển khai lên Hosting qua FTP / FTPS** (hỗ trợ lưu mật khẩu an toàn trong OS Keyring, kiểm tra kết nối, tùy chọn bao gồm thư mục `world` và đẩy luồng dữ liệu máy chủ/modpack lên Hosting có thanh tiến trình & nút Hủy).
3. **Tự Động Xoay Màn Hình Tự Do & Vùng An Toàn Viền Màn Hình Trên Điện Thoại Android**:
   - Mặc định ứng dụng để chế độ **Tự động xoay tự do** (`FullSensor`) — tự động xoay mượt cả 4 chiều ngang/dọc theo đúng hướng tay người dùng cầm điện thoại mà không bị khóa cứng (kể cả khi thanh cài đặt nhanh của máy đang khóa xoay). Bạn cũng có thể nhấn thẻ **Xoay màn hình** trên thanh điều hướng / Menu nổi `HUB` hoặc chọn trong **Cài đặt → Giao diện → Xoay màn hình** giữa **6 chế độ**: **Tự động xoay tự do**, **Theo cài đặt máy**, **Ngang tự động**, **Ngang chuẩn**, **Ngang đảo chiều** và **Màn hình dọc**.
   - Toàn bộ nút điều hướng sát viền, thanh công cụ đỉnh, thanh Dock dưới cùng, hộp thoại, phím cảm ứng Bedrock và nút nổi **`HUB`** tự động căn lề tránh cụm camera đục lỗ / tai thỏ, thanh cử chỉ vuốt cạnh và 4 góc bo cong màn hình để không bao giờ bị che khuất.
4. **Quyền Chạy Ngầm Giữ Mạng, Màn Hình Run Game, Phím Bedrock, Chuột Ảo & Menu Nổi `HUB` Trên Android**:
   - **Cấp quyền giữ mạng khi tắt màn hình & chạy ngầm (`Cài đặt → Quyền Chạy Ngầm, Giữ Mạng & Hiển Thị Nổi`)**: Bật công tắc **Duy trì kết nối mạng khi tắt màn hình & chạy ngầm** và cấp 3 quyền hệ thống (**Bỏ tối ưu pin**, **Hiển thị trên ứng dụng khác**, **Thông báo chạy ngầm**) để giữ `CPU WakeLock` + `Wi-Fi Low-Latency Lock` + `MulticastLock` ổn định cho Server / Mở Port / Phiên game ngay cả khi khóa màn hình hoặc thu nhỏ ứng dụng, kèm nút nổi `THK` gọi nhanh Launcher khi ở ngoài màn hình chính.
   - **Màn hình Game & Phím (`Màn hình Game & Phím` ở cột điều hướng trái hoặc nhấn `Chơi ngay` trên Profile)**: Tích hợp sẵn cụm **Phím di chuyển kiểu Bedrock 3×3** (hỗ trợ đi chéo & lén ở tâm), các nút hành động tùy chỉnh (`Nhảy`, `Tấn công`, `Đặt / Dùng`, `Túi đồ`, ` Đổi tay`, `Chat`, `Góc nhìn`, `F3`, `Tạm dừng`...), **Chuột Ảo** (con trỏ di chuyển cho kho đồ/menu + vuốt xoay camera 360° khi vào thế giới + thanh nút `Trái / Phải / Giữa / Cuộn`), chế độ **Edit Phím** (chạm kéo vị trí từng phím/cụm D-Pad, chỉnh kích thước, độ mờ, hình dạng Tròn/Chữ nhật, gán mã phím GLFW/chuột bất kỳ, thêm/xóa phím hoặc khôi phục mặc định Bedrock), chọn bộ kết xuất đồ họa (**Holy-GL4ES** hoặc **MobileGlues**) và tỷ lệ kết xuất (`40% – 100%`).
   - **Menu Nổi `HUB` trong khi chơi**: Chạm nút nổi **`HUB`** (hoặc kéo thả nút `HUB` tới góc màn hình thuận tay) để bật/ẩn tức thì toàn bộ **HUB UI App** (quản lý Server, Mở Port, Kho Modpack, Nhật ký, Cài đặt) ngay phía trên màn hình game đang chạy mà không cần thoát game; nhấn **Ẩn HUB UI (Về Game)** để quay lại điều khiển trong game ngay lập tức.

---

## 6. Mở Port Theo Mạng Đang Kết Nối & Chia Sẻ Thế Giới Open to LAN (`Chia sẻ LAN`)
1. Mở công cụ **Chia sẻ LAN** ở cột trái và nhấn **Bắt đầu quét LAN** (mặc định bật **Tự động mở Port mạng hiện tại ngay khi phát hiện thế giới LAN**).
2. Vào thế giới Singleplayer trong Minecraft, nhấn `Esc → Open to LAN` (hoặc nhập cổng nội bộ bất kỳ rồi nhấn **Mở Port Mạng Hiện Tại** ở mục 2).
3. Launcher tự động nhận diện mạng đang kết nối để mở cổng cố định (ví dụ `25565`), phát sóng trực tiếp cho bạn bè cùng Wi-Fi / Điểm phát sóng / VPN thấy ngay trong màn hình *Multiplayer*, đồng thời cấp địa chỉ **Online IPv6 trực tiếp** và **Online IPv4** (hoặc đường hầm dự phòng `bore.pub` ở mục 3) để sao chép 1 chạm gửi bạn bè ở mạng khác vào chơi với độ trễ thấp.

---

## 7. Quản Lý Java / Chế Độ Tối Ưu GraalVM, Tự Sửa Lỗi Crash & Tự Động Cập Nhật
- **Chế độ Java Tự động (`Tối ưu (GraalVM)` / `Tiêu chuẩn (Mojang)`)**:
  - Mặc định bật **Tối ưu (GraalVM)**: khi bạn tạo hoặc vào bất kỳ phiên bản Minecraft / gói Modpack nào (hoặc khởi tạo máy chủ cục bộ) mà không gắn cứng đường dẫn Java riêng, Launcher tự động nhận diện thế hệ game để tải & chạy **GraalVM Community JDK** (`GraalVM 17` cho `1.17–1.20.4`, `GraalVM 21` cho `1.20.5–1.21.x`, `GraalVM 25` cho `26.x`), đồng thời tự giữ **`Java 8`** tương thích cho các bản/modpack cũ `<= 1.16.5`.
  - Bạn có thể chuyển nhanh giữa **Tối ưu (GraalVM)** và **Tiêu chuẩn (Mojang)** kèm huy hiệu phiên bản Java mục tiêu ngay tại **Quản lý Java**, **Cài đặt Hệ thống**, **Profile → Cài đặt chung**, **Tạo Profile mới**, **Trạm Cài đặt Modpack**, **Nhập Modpack từ đĩa** và **Tạo Máy chủ mới**, hoặc quét/tải thủ công `Java 8 / 11 / 17 / 21 / 25` từ **Adoptium Temurin**, **Azul Zulu** và **GraalVM** (trên Android `arm64-v8a`, Launcher tự động tải và giải nén **OpenJDK ARM64 Bionic `Java 8 / 17 / 21 / 25`** để cài đặt Forge/NeoForge `--installServer` và chạy **Máy chủ cục bộ** trực tiếp trên điện thoại).
- **Nhật ký chẩn đoán nội bộ (`launcher.log`)**: Mọi cảnh báo/lỗi runtime và thông báo lỗi trên giao diện đều được ghi xoay vòng vào `.TriHienKunLaucher/launcher.log`; bạn có thể nhấn **Sao chép nhật ký** ngay trong mục **Cài đặt** để chép nhanh 250 dòng gần nhất vào Clipboard khi cần kiểm tra sự cố (không phụ thuộc `adb logcat`).
- **Chẩn đoán & Sửa Crash 1 Chạm**: Tự động phân tích cả nhật ký phiên lẫn tệp báo cáo mới nhất trong `crash-reports/`, cung cấp nút tải ngay Mod thư viện còn thiếu từ Modrinth hoặc **Nâng cấp phiên bản Mod Loader** 1 chạm ngay trên Profile.
- **Kiểm tra cập nhật**: Kiểm tra lúc khởi động và kiểm tra thủ công dùng cùng luồng báo lỗi. HTTP 404 từ GitHub `releases/latest` không có nghĩa launcher đang ở phiên bản mới nhất. Tại mốc 07/11/2026, endpoint của repository trả 404 nên chưa kiểm được lượt nâng cấp release thật.
- **Tải bản mới**: Updater so sánh SemVer, chọn artifact đúng hệ điều hành/kiến trúc và đối chiếu SHA-256 theo `digest` hoặc `SHA256SUMS.txt` trên GitHub Release. Không có hash thì từ chối tải cập nhật. **SHA-256 là mã băm, không phải chữ ký số của nhà phát hành.**
- **Áp dụng & Khởi động lại ngay (Windows portable `.exe`)**: Lưu công việc trước khi áp dụng. Launcher kiểm lại hash, chuẩn bị helper rồi chỉ đóng sau khi helper được khởi tạo thành công. Helper tạo staging cạnh executable, kiểm hash lại, lấy khóa độc quyền và dùng `File.Replace` giữ bản cũ ở `.<executable>.<UUID>.backup`. Nếu khởi tạo tiến trình mới báo lỗi, helper cố khôi phục bản cũ và giữ bản lỗi ở `.failed`; các nền tảng/gói khác được chuyển cho hệ điều hành mở, không dùng cơ chế thay `.exe` này.
- **Chẩn đoán cập nhật lỗi**: Xem `cache/updates/apply-<UUID>/config.json.log` và `config.stderr.log` trong thư mục dữ liệu launcher. Không tự xóa `.backup`, `.failed` hoặc `.<executable>.update-lock` khi chưa kiểm tra log và tiến trình liên quan. Khởi tạo tiến trình thành công chưa chứng minh app khỏe; chưa có health handshake hay tự phục hồi sau crash/mất điện.

Xem [trạng thái kiểm thử và các phần chưa xác minh](TESTING.md) trước khi sử dụng những luồng chưa được kiểm chứng đầu-cuối.
