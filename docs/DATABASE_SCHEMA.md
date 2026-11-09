# Đặc Tả Cơ Sở Dữ Liệu, Thư Mục & Keyring — TriHienKun Laucher

Tài liệu này mô tả chính xác cấu trúc thư mục dữ liệu ứng dụng (`AppPaths`), thông số vận hành, cơ chế tự động nâng cấp cột (`ensure_column`) và lược đồ SQL của toàn bộ 7 bảng SQLite trong `crates/thk-core/src/db.rs`, cấu trúc JSON của `AppSettings` và định dạng lưu trữ khóa xác thực trong OS Keyring (`crates/thk-auth/src/token_store.rs`).

---

## 1. Cấu Trúc Thư Mục Dữ Liệu Ứng Dụng (`AppPaths`)

Thư mục gốc được xác định bởi `AppPaths::resolve_default()` thông qua `dirs::config_dir().or_else(dirs::data_dir()).join(".TriHienKunLaucher")`:

- **Windows**: `%APPDATA%\.TriHienKunLaucher` (ví dụ: `C:\Users\<User>\AppData\Roaming\.TriHienKunLaucher`)
- **macOS**: `~/Library/Application Support/.TriHienKunLaucher`
- **Linux**: `$XDG_CONFIG_HOME/.TriHienKunLaucher` (mặc định `~/.config/.TriHienKunLaucher`)
- **Android**: `<AndroidApp::internal_data_path()>/.TriHienKunLaucher` (tương ứng `Context.getFilesDir()`, ví dụ `/data/user/0/com.trihienkun.laucher/files/.TriHienKunLaucher`; kèm thư mục cầu nối `.thk_bridge/` cho chọn tệp SAF, mở trình duyệt và Clipboard hệ thống)

### Cây Thư Mục Chuẩn

```text
.TriHienKunLaucher/
├── launcher.log               # Nhật ký ứng dụng xoay vòng tự động (> 4 MiB đổi sang launcher.prev.log)
├── mobile_controls.json       # Bố cục phím cảm ứng kiểu Bedrock, Chuột Ảo & độ nhạy trên Android
├── thk_laucher.db             # Cơ sở dữ liệu SQLite chính (chế độ WAL)
├── thk_laucher.db-wal         # Nhật ký ghi trước SQLite (Write-Ahead Log)
├── thk_laucher.db-shm         # Bộ nhớ chia sẻ chỉ mục WAL
├── authlib-injector.jar       # Agent JVM cục bộ hỗ trợ hiển thị Skin/Cape Offline
├── .jvm_compat/               # Thư viện thuần Rust libthk_android_ffi.so tắt Bionic Heap Tagging cho JVM trên Android
├── icons/                     # Biểu tượng ứng dụng sinh tự động (icon.png, icon.ico)
├── backgrounds/               # Ảnh nền tùy chỉnh (wallpaper.*) & nhạc nền riêng (custom_bgm.*)
├── cache/                     # Bộ nhớ đệm tải gói Modpack, bản cập nhật & vanilla_steve_{wide,slim}.png
│   └── updates/               # Gói cập nhật đã kiểm SHA-256 & thư mục apply-<UUID>/ (apply_update.ps1, config.json, *.log)
├── account_skins/             # Tệp PNG skin 64x64 đã chuẩn hóa (cục bộ + đệm Mojang Cloud remote_*.png)
├── account_capes/             # Tệp PNG áo choàng Cape đã chuẩn hóa (cục bộ + đệm Mojang Cloud remote_*.png)
├── account_elytras/           # Tệp PNG cánh Elytra đã chuẩn hóa
├── runtimes/                  # Các bản Java JRE/JDK do Launcher tự động tải & quản lý
├── instances/                 # Thư mục mặc định chứa từng Profile Minecraft độc lập
│   └── <slug>/                # mods/, shaderpacks/, resourcepacks/, saves/, config/...
├── servers/                   # Thư mục mặc định chứa các Máy chủ Minecraft cục bộ
│   └── <slug>/                # server.jar, eula.txt, server.properties, plugins/, mods/, world/...
└── launcher/                  # Kho tài nguyên dùng chung giữa mọi Profile (tránh tải trùng lặp)
    ├── versions/              # Manifest JSON & client.jar gốc theo phiên bản
    ├── libraries/             # Kho thư viện Maven dùng chung + thư mục natives giải nén
    └── assets/                # Kho âm thanh/ngôn ngữ/tài nguyên Mojang (indexes/ & objects/)
```

### Đường Dẫn Staging, Recovery & Helper Tạm Thời Trên Đĩa

- **Staging cài đặt nội dung theo batch (`thk-mods::install`)**: Tạo thư mục tạm `<dest_dir>/.thk-install-<UUID>/` ngay bên trong `mods/`, `shaderpacks/` hoặc `resourcepacks/` của Profile (gồm tệp tải tạm theo chỉ số, thư mục `backup/` và `recovery.json`). Tự động xóa khi hoàn tất hoặc hủy/lỗi trước commit; giữ nguyên nếu rollback thất bại để đối chiếu phục hồi thủ công.
- **Staging cài/nhập Modpack (`thk-app::pack_install`)**: Tạo `<parent>/.thk-pack-<UUID>/` cạnh thư mục đích của Profile (gồm `payload/` và `recovery.json` ghi nhận `destination`, `files`, `directories`, `profile`). Tự động xóa khi commit thành công hoặc hủy/lỗi trước commit; giữ nguyên nếu quá trình hoàn tác tệp báo lỗi.
- **Helper cập nhật Windows portable (`thk-app::updater` + `apply_update.ps1`)**:
  - Nhật ký & cấu hình từng lượt áp dụng: `cache/updates/apply-<UUID>/` chứa `apply_update.ps1`, `config.json`, `config.json.log` và `config.stderr.log`.
  - Cạnh tệp `.exe` đang chạy trong lúc thay thế: khóa độc quyền `.<exe>.update-lock`, tệp chuẩn bị `.<exe>.<UUID>.staged`, bản sao lưu `.<exe>.<UUID>.backup`, và `.<exe>.<UUID>.failed` nếu tiến trình mới không khởi chạy được và phải khôi phục bản cũ.

---

## 2. Cấu Hình Kết Nối SQLite & Nâng Cấp Lược Đồ Tự Động

Khi mở tệp `thk_laucher.db`, `Database::open` thiết lập ngay bộ chỉ thị PRAGMA sau:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

Sau khi thực thi `CREATE TABLE IF NOT EXISTS` và khởi tạo các `TRIGGER` dọn dẹp dữ liệu liên kết, hàm `ensure_column` kiểm tra `PRAGMA table_info(...)` để tự động bổ sung các cột mới (`window_width`, `window_height`, `fullscreen` trên bảng `profiles`; `cape_url`, `oauth_client_id` trên bảng `accounts`) một cách bất biến (`idempotent`) đối với các cơ sở dữ liệu tạo từ phiên bản trước.

Khi đọc `kv_state['app_settings']`, `Database::load_settings` áp dụng hai quy tắc bảo toàn cấu hình:
1. **Không tự xóa cấu hình khi JSON hỏng**: Nếu chuỗi JSON trong `app_settings` không giải mã được, hàm trả về lỗi `ThkError::Json` để `LauncherAppState::initialize` dừng khởi chạy và hiện hộp thoại báo lỗi thay vì âm thầm ghi đè cấu hình của người dùng bằng giá trị mặc định.
2. **Di trú kho cập nhật mặc định cũ**: Nếu `github_repo` mang giá trị mặc định cũ `"trihienkun/trihienkun-laucher"` (không phân biệt hoa thường), hệ thống tự cập nhật sang `"PhamTriHien/TriHienKun_Laucher"` trong `kv_state` mà vẫn giữ nguyên mọi tùy chọn khác cũng như các kho GitHub tùy chỉnh do người dùng tự đặt.

---

## 3. Lược Đồ Chi Tiết 7 Bảng SQLite (`init_schema`)

### 3.1. Bảng `kv_state`
Lưu trữ cấu hình toàn cục (`app_settings`), cấu hình kết nối FTP/FTPS không chứa mật khẩu của từng máy chủ (`server_ftp_cfg:<server_id>` → JSON `FtpDeployConfig`) và trạng thái lựa chọn gần nhất (`selected_profile_id`, `selected_account_id`).

```sql
CREATE TABLE IF NOT EXISTS kv_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

### 3.2. Bảng `profiles`
Lưu trữ danh sách Profile trò chơi, cấu hình Mod Loader, bộ nhớ RAM, tham số JVM, kích thước cửa sổ / toàn màn hình và bộ đếm thống kê tổng hợp.

```sql
CREATE TABLE IF NOT EXISTS profiles (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    game_version        TEXT NOT NULL,
    loader              TEXT NOT NULL,               -- 'vanilla' | 'fabric' | 'quilt' | 'forge' | 'neoforge'
    loader_version      TEXT,
    instance_path       TEXT NOT NULL,
    is_custom_path      INTEGER NOT NULL DEFAULT 0,  -- 0: mặc định trong instances/, 1: thư mục tùy chọn
    ram_mb              INTEGER NOT NULL DEFAULT 4096,
    java_path           TEXT,                        -- NULL: tự động phân giải GraalVM / Mojang Java theo AppSettings::java_runtime_mode
    jvm_args            TEXT NOT NULL DEFAULT '',
    auto_perf_mods      INTEGER NOT NULL DEFAULT 0,  -- Tự động cài gói tối ưu FPS cho Fabric
    icon_url            TEXT,
    window_width        INTEGER,                     -- Chiều rộng cửa sổ game tùy chỉnh (px)
    window_height       INTEGER,                     -- Chiều cao cửa sổ game tùy chỉnh (px)
    fullscreen          INTEGER NOT NULL DEFAULT 0,  -- 1: Khởi chạy toàn màn hình (--fullscreen)
    total_play_seconds  INTEGER NOT NULL DEFAULT 0,
    session_count       INTEGER NOT NULL DEFAULT 0,
    crash_count         INTEGER NOT NULL DEFAULT 0,
    last_played_at      INTEGER,                     -- Unix timestamp (UTC seconds)
    created_at          INTEGER NOT NULL             -- Unix timestamp (UTC seconds)
);
```

> **Cơ chế bảo toàn dữ liệu nguyên tử & Nhân bản Profile**: Câu lệnh `upsert_profile` chỉ cập nhật các cột cấu hình (`name`, `game_version`, `loader`, `loader_version`, `instance_path`, `is_custom_path`, `ram_mb`, `java_path`, `jvm_args`, `auto_perf_mods`, `icon_url`, `window_width`, `window_height`, `fullscreen`), trong khi `record_play_session` cộng dồn trực tiếp trên SQL (`total_play_seconds = total_play_seconds + ?2`, `session_count = session_count + 1`, `crash_count = crash_count + ?3`). Khi nhân bản Profile, `clone_installed_content_for_profile` sao chép toàn bộ bản ghi gói đã cài đặt sang `profile_id` mới.

### 3.3. Bảng `daily_play_stats`
Lưu trữ thời lượng chơi theo từng ngày cho biểu đồ phân tích 14 ngày gần nhất; tự động xóa dây chuyền (`ON DELETE CASCADE`) khi xóa Profile.

```sql
CREATE TABLE IF NOT EXISTS daily_play_stats (
    profile_id      TEXT NOT NULL,
    day_yyyy_mm_dd  TEXT NOT NULL,                   -- Định dạng 'YYYY-MM-DD' (UTC)
    seconds_played  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (profile_id, day_yyyy_mm_dd),
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
);
```

### 3.4. Bảng `accounts`
Lưu trữ siêu dữ liệu công khai của tài khoản người chơi (không lưu mật khẩu hay OAuth token nhạy cảm trong SQLite).

```sql
CREATE TABLE IF NOT EXISTS accounts (
    id                  TEXT PRIMARY KEY,
    username            TEXT NOT NULL,
    uuid                TEXT NOT NULL,
    account_type        TEXT NOT NULL,               -- 'offline' | 'microsoft'
    skin_variant        TEXT NOT NULL DEFAULT 'wide',-- 'wide' | 'slim'
    skin_url            TEXT,                        -- URL Skin từ Mojang Profile API
    cape_url            TEXT,                        -- URL Active Cape từ Mojang Profile API
    custom_skin_path    TEXT,
    custom_cape_path    TEXT,
    custom_elytra_path  TEXT,
    token_expires_at    INTEGER,                     -- Unix timestamp hết hạn của Minecraft access token
    oauth_client_id     TEXT,                        -- Azure Client ID riêng nếu đăng nhập qua Loopback PKCE (NULL: Live Public Client)
    created_at          INTEGER NOT NULL
);
```

### 3.5. Bảng `servers`
Lưu trữ cấu hình các máy chủ Minecraft chuyên dụng cục bộ.

```sql
CREATE TABLE IF NOT EXISTS servers (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    engine          TEXT NOT NULL,                   -- 'paper' | 'purpur' | 'folia' | 'vanilla' | 'fabric' | 'forge' | 'neoforge' | 'mohist' | 'arclight' | 'sponge'
    game_version    TEXT NOT NULL,
    server_dir      TEXT NOT NULL,
    is_custom_dir   INTEGER NOT NULL DEFAULT 0,
    port            INTEGER NOT NULL DEFAULT 25565,
    ram_min_mb      INTEGER NOT NULL DEFAULT 1024,
    ram_max_mb      INTEGER NOT NULL DEFAULT 4096,
    java_path       TEXT,
    jvm_flags       TEXT NOT NULL DEFAULT '',
    jar_file        TEXT NOT NULL DEFAULT 'server.jar',
    created_at      INTEGER NOT NULL
);
```

### 3.6. Bảng `server_bookmarks`
Lưu trữ danh sách máy chủ Multiplayer yêu thích để Ping trạng thái trực tiếp và vào nhanh (`QuickPlay`).

```sql
CREATE TABLE IF NOT EXISTS server_bookmarks (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    address     TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);
```

### 3.7. Bảng `installed_content` & Triggers Dọn Dẹp Tự Động
Theo dõi siêu dữ liệu và mã băm `SHA-1` của từng Mod, Shader, ResourcePack hoặc Plugin đã cài đặt cho từng Profile (`profile_id = <id>`) hoặc Máy chủ cục bộ (`profile_id = 'server:' || <server_id>`).

```sql
CREATE TABLE IF NOT EXISTS installed_content (
    profile_id      TEXT NOT NULL,
    content_kind    TEXT NOT NULL,                   -- 'mod' | 'modpack' | 'shader' | 'resourcepack' | 'plugin'
    file_name       TEXT NOT NULL,                   -- Tên tệp gốc không kèm hậu tố '.disabled'
    sha1            TEXT NOT NULL,
    provider        TEXT NOT NULL,                   -- 'modrinth' | 'curseforge' | 'ftb' | 'technic' | 'spiget' | 'local'
    project_id      TEXT NOT NULL,
    version_id      TEXT NOT NULL,
    title           TEXT NOT NULL,
    version_number  TEXT NOT NULL,
    icon_url        TEXT,
    enabled         INTEGER NOT NULL DEFAULT 1,
    installed_at    INTEGER NOT NULL,
    PRIMARY KEY (profile_id, content_kind, file_name)
);

CREATE TRIGGER IF NOT EXISTS trg_profiles_delete_installed_content
AFTER DELETE ON profiles
BEGIN
    DELETE FROM installed_content WHERE profile_id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_servers_delete_installed_content
AFTER DELETE ON servers
BEGIN
    DELETE FROM installed_content
    WHERE profile_id = OLD.id OR profile_id = ('server:' || OLD.id);
END;
```

> **Giao dịch ghi metadata nguyên tử (`Database::commit_installed_content` & `Database::upsert_profile_with_installed_content`)**:
> 1. Khi cài đặt hoặc cập nhật gói nội dung lẻ qua `thk-mods::install`, toàn bộ lô bản ghi được thực thi trong một `rusqlite::Transaction` duy nhất: kiểm tra xung đột quyền sở hữu tên tệp không phân biệt hoa thường (`file_name COLLATE NOCASE`), dọn tên tệp cũ của chính `(provider, project_id)` đó (so khớp `COLLATE NOCASE` mà không ảnh hưởng dự án của nhà cung cấp khác trùng `project_id`), rồi `UPSERT` bản ghi mới.
> 2. Khi cài đặt từ 4 kho Modpack hoặc nhập `.mrpack`/`.zip` từ đĩa qua `PackInstallation::commit`, `Database::upsert_profile_with_installed_content` lưu cả dòng `profiles` lẫn toàn bộ danh sách `installed_content` trích xuất từ gói vào chung một giao dịch SQLite nguyên tử.

---

## 4. Cấu Trúc JSON `AppSettings` (Khóa `kv_state['app_settings']`)

| Trường | Kiểu Rust | Giá Trị Mặc Định | Mô Tả |
| :--- | :--- | :--- | :--- |
| `language` | `Language` | `"vi"` | Ngôn ngữ hiển thị (`"vi"` hoặc `"en"`). |
| `theme` | `ThemeMode` | `"dark"` | Giao diện màu (`"dark"` hoặc `"light"`). |
| `accent_preset` | `AccentPreset` | `"emerald"` | Bảng màu nhấn chủ đạo (`"emerald"`, `"cyan"`, `"blue"`, `"violet"`, `"amber"`, `"crimson"`). |
| `font_scale` | `f32` | `1.0` (`0.78` khi khởi tạo lần đầu trên Android) | Tỷ lệ phóng đại cỡ chữ và giao diện (`0.50..=2.00`, tương ứng `50%..=200%`), tự động giới hạn an toàn qua `sanitize_font_scale`. |
| `java_runtime_mode` | `JavaRuntimeMode` | `"optimal_graal_vm"` | Chế độ tự động chọn Java khi không gắn `java_path` riêng (`"optimal_graal_vm"` tự động chọn & cài `GraalVM 17/21/25` hoặc `Java 8` tương thích; `"mojang_official"` dùng Mojang JRE tiêu chuẩn). |
| `close_behavior` | `CloseBehavior` | `"ask"` | Hành vi khi đóng cửa sổ (`"ask"`, `"minimize_to_tray"`, `"quit"`). |
| `hide_launcher_on_launch` | `bool` | `false` | Tự động ẩn cửa sổ Launcher khi Minecraft khởi chạy và hiện lại khi thoát game. |
| `open_logs_on_launch` | `bool` | `false` | Tự động chuyển sang thẻ Nhật ký trực tiếp (`Logs`) khi khởi chạy game. |
| `high_process_priority` | `bool` | `false` | Cấp mức ưu tiên CPU cao (`ABOVE_NORMAL_PRIORITY_CLASS`) cho tiến trình JVM trên Windows. |
| `low_ram_mode` | `bool` | `false` | Bật Chế độ Siêu tiết kiệm RAM cho Launcher (tắt hoạt ảnh rảnh, thu nhỏ/giải phóng bộ đệm GPU offscreen, giới hạn cache ảnh & log, hạ trần tải đồng thời `<= 4`). |
| `default_ram_mb` | `u32` | `4096` | Dung lượng RAM mặc định (MB) khi tạo Profile mới. |
| `max_concurrent_downloads` | `usize` | `12` | Số luồng tải song song tối đa (`2..=32`) áp dụng trực tiếp cho `DownloadManager`. |
| `discord_rpc_enabled` | `bool` | `true` | Bật/tắt đồng bộ trạng thái Discord Rich Presence. |
| `sfx_enabled` | `bool` | `true` | Bật/tắt âm thanh phản hồi giao diện (`rodio`). |
| `music_enabled` | `bool` | `true` | Bật/tắt nhạc nền Stereo đa nhạc cụ tích hợp sẵn hoặc tệp nhạc tùy chỉnh (`custom_bgm.mp3/ogg/wav`); tự tạm dừng khi đang chạy Minecraft. |
| `music_volume` | `f32` | `0.35` | Âm lượng nhạc nền (`0.05..=1.0`). |
| `custom_background_path` | `Option<PathBuf>` | `null` | Đường dẫn ảnh nền tùy chỉnh lưu trong `backgrounds/`. |
| `curseforge_api_key` | `Option<String>` | `null` | Khóa API CurseForge cá nhân tùy chọn (`x-api-key`). |
| `auto_check_updates` | `bool` | `true` | Tự động kiểm tra bản phát hành mới trên GitHub khi mở ứng dụng. |
| `github_repo` | `String` | `"PhamTriHien/TriHienKun_Laucher"` | Kho lưu trữ GitHub dùng để kiểm tra cập nhật. Giá trị mặc định cũ được di trú khi đọc cấu hình; kho tùy chỉnh được giữ nguyên. |
| `discord_client_id` | `Option<String>` | `null` | Discord Application ID tùy chỉnh (nếu để trống dùng ID tích hợp sẵn). |
| `mobile_orientation` | `MobileOrientationMode` | `"sensor_landscape"` | Chế độ xoay màn hình trên Android (`"sensor_landscape"`, `"landscape"`, `"reverse_landscape"`, `"portrait"`, `"full_sensor"`). |
| `mobile_keep_alive` | `bool` | `true` | Bật dịch vụ tiền cảnh `ThkKeepAliveService` giữ `PARTIAL_WAKE_LOCK`, `WIFI_MODE_FULL_LOW_LATENCY` và `MulticastLock` khi tắt màn hình hoặc chạy ngầm trên Android. |
| `mobile_floating_overlay` | `bool` | `true` | Bật nút nổi gọi nhanh Launcher (`SYSTEM_ALERT_WINDOW`) khi ứng dụng thu nhỏ ra ngoài màn hình chính Android. |
| `mobile_renderer` | `MobileRendererMode` | `"holy_gl4es"` | Bộ kết xuất đồ họa Minecraft Java trên Android (`"holy_gl4es"` OpenGL 2.1 tương thích rộng hoặc `"mobile_glues"` OpenGL 3.2+ hiệu năng cao). |
| `mobile_render_scale_pct` | `u8` | `85` | Tỷ lệ độ phân giải kết xuất khung hình game trên Android (`40..=100%`, giới hạn an toàn qua `sanitize_mobile_render_scale_pct`). |
| `setup_completed` | `bool` | `false` | Đánh dấu hoàn tất hộp thoại thiết lập lần đầu. |

---

## 5. Lưu Trữ Khóa Xác Thực Trong OS Keyring (`TokenStore`)

Mọi khóa xác thực nhạy cảm đều **tách biệt hoàn toàn khỏi tệp SQLite** và được lưu tách làm hai mục độc lập trong kho khóa gốc của hệ điều hành (tránh vượt trần 2.560 byte UTF-16LE `CredentialBlob` của Windows Credential Manager khi JWT và Refresh Token đều dài):

- **Service Name**: `com.trihienkun.laucher` (`thk_core::APP_ID`)
- **Account & FTP Keys**:
  - `account:<account_id>:mc` — lưu chuỗi `mc_access_token` (JWT truy cập Minecraft Services).
  - `account:<account_id>:ms` — lưu chuỗi `ms_refresh_token` (OAuth2 Refresh Token của Microsoft, nếu có).
  - `server-ftp:<server_id>` — lưu mật khẩu FTP/FTPS triển khai Hosting của từng máy chủ cục bộ (`thk-server::ftp_deploy`, không bao giờ ghi mật khẩu vào bảng `kv_state`).
- **Tương thích ngược tự động**: Nếu phát hiện khóa định dạng cũ `account:<account_id>` (chứa JSON `StoredAuthTokens`), `TokenStore::load_tokens` tự động đọc, tách sang cặp khóa `:mc` / `:ms` và xóa mục cũ.
- Khi người dùng xóa tài khoản khỏi giao diện, `TokenStore::delete_tokens` xóa sạch cả `:mc`, `:ms` và khóa cũ (nếu còn) khỏi OS Keyring đồng thời xóa bản ghi trong bảng `accounts`. Khi xóa máy chủ, ứng dụng cũng xóa đồng thời cả `server_ftp_cfg:<server_id>` trong SQLite lẫn khóa `server-ftp:<server_id>` trong OS Keyring.
