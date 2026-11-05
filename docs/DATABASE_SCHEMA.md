# Đặc Tả Cơ Sở Dữ Liệu, Thư Mục & Keyring — TriHienKun Laucher

Tài liệu này mô tả chính xác cấu trúc thư mục dữ liệu ứng dụng (`AppPaths`), thông số vận hành, cơ chế tự động nâng cấp cột (`ensure_column`) và lược đồ SQL của toàn bộ 7 bảng SQLite trong `crates/thk-core/src/db.rs`, cấu trúc JSON của `AppSettings` và định dạng lưu trữ khóa xác thực trong OS Keyring (`crates/thk-auth/src/token_store.rs`).

---

## 1. Cấu Trúc Thư Mục Dữ Liệu Ứng Dụng (`AppPaths`)

Thư mục gốc được xác định bởi `AppPaths::resolve_default()` thông qua `dirs::config_dir().or_else(dirs::data_dir()).join(".TriHienKunLaucher")`:

- **Windows**: `%APPDATA%\.TriHienKunLaucher` (ví dụ: `C:\Users\<User>\AppData\Roaming\.TriHienKunLaucher`)
- **macOS**: `~/Library/Application Support/.TriHienKunLaucher`
- **Linux**: `$XDG_CONFIG_HOME/.TriHienKunLaucher` (mặc định `~/.config/.TriHienKunLaucher`)

### Cây Thư Mục Chuẩn

```text
.TriHienKunLaucher/
├── thk_laucher.db             # Cơ sở dữ liệu SQLite chính (chế độ WAL)
├── thk_laucher.db-wal         # Nhật ký ghi trước SQLite (Write-Ahead Log)
├── thk_laucher.db-shm         # Bộ nhớ chia sẻ chỉ mục WAL
├── authlib-injector.jar       # Agent JVM cục bộ hỗ trợ hiển thị Skin/Cape Offline
├── icons/                     # Biểu tượng ứng dụng sinh tự động (icon.png, icon.ico)
├── backgrounds/               # Ảnh nền tùy chỉnh (wallpaper.*) & nhạc nền riêng (custom_bgm.*)
├── cache/                     # Bộ nhớ đệm tải gói Modpack & bản cập nhật (cache/updates/)
├── account_skins/             # Tệp PNG skin 64x64 đã chuẩn hóa (cục bộ + đệm Mojang Cloud remote_*.png)
├── account_capes/             # Tệp PNG áo choàng Cape đã chuẩn hóa (cục bộ + đệm Mojang Cloud remote_*.png)
├── account_elytras/           # Tệp PNG cánh Elytra đã chuẩn hóa
├── runtimes/                  # Các bản Java JRE/JDK do Launcher tự động tải & quản lý
├── instances/                 # Thư mục mặc định chứa từng Profile Minecraft độc lập
│   └── <slug>-<id8>/          # mods/, shaderpacks/, resourcepacks/, saves/, config/...
├── servers/                   # Thư mục mặc định chứa các Máy chủ Minecraft cục bộ
│   └── <slug>-<id8>/          # server.jar, eula.txt, server.properties, plugins/, mods/, world/...
└── launcher/                  # Kho tài nguyên dùng chung giữa mọi Profile (tránh tải trùng lặp)
    ├── versions/              # Manifest JSON & client.jar gốc theo phiên bản
    ├── libraries/             # Kho thư viện Maven dùng chung + thư mục natives giải nén
    └── assets/                # Kho âm thanh/ngôn ngữ/tài nguyên Mojang (indexes/ & objects/)
```

---

## 2. Cấu Hình Kết Nối SQLite & Nâng Cấp Lược Đồ Tự Động

Khi mở tệp `thk_laucher.db`, `Database::open` thiết lập ngay bộ chỉ thị PRAGMA sau:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

Sau khi thực thi `CREATE TABLE IF NOT EXISTS`, hàm `ensure_column` kiểm tra `PRAGMA table_info(...)` để tự động bổ sung các cột mới (`window_width`, `window_height`, `fullscreen` trên bảng `profiles` và `cape_url` trên bảng `accounts`) một cách bất biến (`idempotent`) đối với các cơ sở dữ liệu tạo từ phiên bản trước.

---

## 3. Lược Đồ Chi Tiết 7 Bảng SQLite (`init_schema`)

### 3.1. Bảng `kv_state`
Lưu trữ cấu hình toàn cục (`app_settings`) và trạng thái lựa chọn gần nhất (`selected_profile_id`, `selected_account_id`).

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
    java_path           TEXT,                        -- NULL: tự động chọn Mojang Java chuẩn
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

### 3.7. Bảng `installed_content`
Theo dõi siêu dữ liệu và mã băm `SHA-1` của từng Mod, Shader, ResourcePack hoặc Plugin đã cài đặt cho từng Profile.

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
```

---

## 4. Cấu Trúc JSON `AppSettings` (Khóa `kv_state['app_settings']`)

| Trường | Kiểu Rust | Giá Trị Mặc Định | Mô Tả |
| :--- | :--- | :--- | :--- |
| `language` | `Language` | `"vi"` | Ngôn ngữ hiển thị (`"vi"` hoặc `"en"`). |
| `theme` | `ThemeMode` | `"dark"` | Giao diện màu (`"dark"` hoặc `"light"`). |
| `accent_preset` | `AccentPreset` | `"emerald"` | Bảng màu nhấn chủ đạo (`"emerald"`, `"cyan"`, `"blue"`, `"violet"`, `"amber"`, `"crimson"`). |
| `close_behavior` | `CloseBehavior` | `"ask"` | Hành vi khi đóng cửa sổ (`"ask"`, `"minimize_to_tray"`, `"quit"`). |
| `hide_launcher_on_launch` | `bool` | `false` | Tự động ẩn cửa sổ Launcher khi Minecraft khởi chạy và hiện lại khi thoát game. |
| `open_logs_on_launch` | `bool` | `false` | Tự động chuyển sang thẻ Nhật ký trực tiếp (`Logs`) khi khởi chạy game. |
| `high_process_priority` | `bool` | `false` | Cấp mức ưu tiên CPU cao (`ABOVE_NORMAL_PRIORITY_CLASS`) cho tiến trình JVM trên Windows. |
| `default_ram_mb` | `u32` | `4096` | Dung lượng RAM mặc định (MB) khi tạo Profile mới. |
| `max_concurrent_downloads` | `usize` | `12` | Số luồng tải song song tối đa (`2..=32`) áp dụng trực tiếp cho `DownloadManager`. |
| `discord_rpc_enabled` | `bool` | `true` | Bật/tắt đồng bộ trạng thái Discord Rich Presence. |
| `sfx_enabled` | `bool` | `true` | Bật/tắt âm thanh phản hồi giao diện (`rodio`). |
| `music_enabled` | `bool` | `false` | Bật/tắt nhạc nền Ambient hoặc tệp nhạc tùy chỉnh (`custom_bgm.mp3/ogg/wav`). |
| `music_volume` | `f32` | `0.35` | Âm lượng nhạc nền (`0.05..=1.0`). |
| `custom_background_path` | `Option<PathBuf>` | `null` | Đường dẫn ảnh nền tùy chỉnh lưu trong `backgrounds/`. |
| `curseforge_api_key` | `Option<String>` | `null` | Khóa API CurseForge cá nhân tùy chọn (`x-api-key`). |
| `auto_check_updates` | `bool` | `true` | Tự động kiểm tra bản phát hành mới trên GitHub khi mở ứng dụng. |
| `github_repo` | `String` | `"trihienkun/trihienkun-laucher"` | Kho lưu trữ GitHub dùng để kiểm tra cập nhật. |
| `discord_client_id` | `Option<String>` | `null` | Discord Application ID tùy chỉnh (nếu để trống dùng ID tích hợp sẵn). |
| `setup_completed` | `bool` | `false` | Đánh dấu hoàn tất hộp thoại thiết lập lần đầu. |

---

## 5. Lưu Trữ Khóa Xác Thực Trong OS Keyring (`TokenStore`)

Mọi khóa xác thực nhạy cảm đều **tách biệt hoàn toàn khỏi tệp SQLite** và được lưu trong kho khóa gốc của hệ điều hành:

- **Service Name**: `com.trihienkun.laucher` (`thk_core::APP_ID`)
- **Account Key**: `account:<account_id>`
- **Chuỗi JSON được mã hóa lưu trữ (`StoredAuthTokens`)**:
  ```json
  {
    "mc_access_token": "<JWT truy cập Minecraft Services>",
    "ms_refresh_token": "<OAuth2 Refresh Token của Microsoft>"
  }
  ```
- Khi người dùng xóa tài khoản khỏi giao diện, `TokenStore::delete_tokens` xóa vĩnh viễn mục tương ứng khỏi OS Keyring đồng thời xóa bản ghi trong bảng `accounts`.
