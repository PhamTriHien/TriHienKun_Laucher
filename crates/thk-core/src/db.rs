use crate::error::ThkResult;
use crate::models::{
    Account, AccountType, AppSettings, ContentKind, DailyPlayStat, GameProfile,
    InstalledContentRecord, LoaderType, ManagedServer, ProviderSource, ServerBookmark,
    ServerEngine, SkinVariant,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> ThkResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    pub fn open_in_memory() -> ThkResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv_state (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profiles (
                id                  TEXT PRIMARY KEY,
                name                TEXT NOT NULL,
                game_version        TEXT NOT NULL,
                loader              TEXT NOT NULL,
                loader_version      TEXT,
                instance_path       TEXT NOT NULL,
                is_custom_path      INTEGER NOT NULL DEFAULT 0,
                ram_mb              INTEGER NOT NULL DEFAULT 4096,
                java_path           TEXT,
                jvm_args            TEXT NOT NULL DEFAULT '',
                auto_perf_mods      INTEGER NOT NULL DEFAULT 0,
                icon_url            TEXT,
                total_play_seconds  INTEGER NOT NULL DEFAULT 0,
                session_count       INTEGER NOT NULL DEFAULT 0,
                crash_count         INTEGER NOT NULL DEFAULT 0,
                last_played_at      INTEGER,
                created_at          INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS daily_play_stats (
                profile_id      TEXT NOT NULL,
                day_yyyy_mm_dd  TEXT NOT NULL,
                seconds_played  INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (profile_id, day_yyyy_mm_dd),
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS accounts (
                id                  TEXT PRIMARY KEY,
                username            TEXT NOT NULL,
                uuid                TEXT NOT NULL,
                account_type        TEXT NOT NULL,
                skin_variant        TEXT NOT NULL DEFAULT 'wide',
                skin_url            TEXT,
                custom_skin_path    TEXT,
                custom_cape_path    TEXT,
                custom_elytra_path  TEXT,
                token_expires_at    INTEGER,
                created_at          INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS servers (
                id              TEXT PRIMARY KEY,
                name            TEXT NOT NULL,
                engine          TEXT NOT NULL,
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

            CREATE TABLE IF NOT EXISTS server_bookmarks (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                address     TEXT NOT NULL,
                created_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS installed_content (
                profile_id      TEXT NOT NULL,
                content_kind    TEXT NOT NULL,
                file_name       TEXT NOT NULL,
                sha1            TEXT NOT NULL,
                provider        TEXT NOT NULL,
                project_id      TEXT NOT NULL,
                version_id      TEXT NOT NULL,
                title           TEXT NOT NULL,
                version_number  TEXT NOT NULL,
                icon_url        TEXT,
                enabled         INTEGER NOT NULL DEFAULT 1,
                installed_at    INTEGER NOT NULL,
                PRIMARY KEY (profile_id, content_kind, file_name)
            );",
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> ThkResult<AppSettings> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM kv_state WHERE key = 'app_settings'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match raw {
            Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save_settings(&self, settings: &AppSettings) -> ThkResult<()> {
        let json = serde_json::to_string(settings)?;
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO kv_state (key, value) VALUES ('app_settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> ThkResult<Option<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn
            .query_row("SELECT value FROM kv_state WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_kv(&self, key: &str, value: Option<&str>) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        match value {
            Some(val) => {
                conn.execute(
                    "INSERT INTO kv_state (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, val],
                )?;
            }
            None => {
                conn.execute("DELETE FROM kv_state WHERE key = ?1", params![key])?;
            }
        }
        Ok(())
    }

    pub fn list_profiles(&self) -> ThkResult<Vec<GameProfile>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, game_version, loader, loader_version, instance_path,
                    is_custom_path, ram_mb, java_path, jvm_args, auto_perf_mods, icon_url,
                    total_play_seconds, session_count, crash_count, last_played_at, created_at
             FROM profiles
             ORDER BY COALESCE(last_played_at, 0) DESC, created_at DESC",
        )?;
        let rows = stmt.query_map([], map_profile_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_profile(&self, id: &str) -> ThkResult<Option<GameProfile>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn
            .query_row(
                "SELECT id, name, game_version, loader, loader_version, instance_path,
                        is_custom_path, ram_mb, java_path, jvm_args, auto_perf_mods, icon_url,
                        total_play_seconds, session_count, crash_count, last_played_at, created_at
                 FROM profiles WHERE id = ?1",
                params![id],
                map_profile_row,
            )
            .optional()?)
    }

    pub fn upsert_profile(&self, p: &GameProfile) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO profiles (
                id, name, game_version, loader, loader_version, instance_path,
                is_custom_path, ram_mb, java_path, jvm_args, auto_perf_mods, icon_url,
                total_play_seconds, session_count, crash_count, last_played_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                game_version = excluded.game_version,
                loader = excluded.loader,
                loader_version = excluded.loader_version,
                instance_path = excluded.instance_path,
                is_custom_path = excluded.is_custom_path,
                ram_mb = excluded.ram_mb,
                java_path = excluded.java_path,
                jvm_args = excluded.jvm_args,
                auto_perf_mods = excluded.auto_perf_mods,
                icon_url = excluded.icon_url",
            params![
                p.id,
                p.name,
                p.game_version,
                p.loader.as_str(),
                p.loader_version,
                p.instance_path.to_string_lossy().as_ref(),
                p.is_custom_path as i32,
                p.ram_mb,
                p.java_path.as_ref().map(|j| j.to_string_lossy().to_string()),
                p.jvm_args,
                p.auto_perf_mods as i32,
                p.icon_url,
                p.total_play_seconds as i64,
                p.session_count,
                p.crash_count,
                p.last_played_at,
                p.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_profile(&self, id: &str) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM installed_content WHERE profile_id = ?1", params![id])?;
        conn.execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn record_play_session(&self, profile_id: &str, duration_secs: u64, crashed: bool) -> ThkResult<()> {
        let now = Utc::now();
        let ts = now.timestamp();
        let day = now.format("%Y-%m-%d").to_string();
        let crash_inc = if crashed { 1 } else { 0 };
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE profiles
             SET total_play_seconds = total_play_seconds + ?2,
                 session_count = session_count + 1,
                 crash_count = crash_count + ?3,
                 last_played_at = ?4
             WHERE id = ?1",
            params![profile_id, duration_secs as i64, crash_inc, ts],
        )?;
        conn.execute(
            "INSERT INTO daily_play_stats (profile_id, day_yyyy_mm_dd, seconds_played)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(profile_id, day_yyyy_mm_dd) DO UPDATE SET
                seconds_played = seconds_played + excluded.seconds_played",
            params![profile_id, day, duration_secs as i64],
        )?;
        Ok(())
    }

    pub fn get_daily_play_stats(&self, profile_id: &str, limit_days: u32) -> ThkResult<Vec<DailyPlayStat>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT day_yyyy_mm_dd, seconds_played
             FROM daily_play_stats
             WHERE profile_id = ?1
             ORDER BY day_yyyy_mm_dd DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![profile_id, limit_days], |r| {
            let secs: i64 = r.get(1)?;
            Ok(DailyPlayStat {
                day_yyyy_mm_dd: r.get(0)?,
                seconds_played: secs.max(0) as u64,
            })
        })?;
        let mut out = rows.collect::<Result<Vec<_>, _>>()?;
        out.reverse();
        Ok(out)
    }

    pub fn list_accounts(&self) -> ThkResult<Vec<Account>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, username, uuid, account_type, skin_variant, skin_url,
                    custom_skin_path, custom_cape_path, custom_elytra_path,
                    token_expires_at, created_at
             FROM accounts
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], map_account_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_account(&self, id: &str) -> ThkResult<Option<Account>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn
            .query_row(
                "SELECT id, username, uuid, account_type, skin_variant, skin_url,
                        custom_skin_path, custom_cape_path, custom_elytra_path,
                        token_expires_at, created_at
                 FROM accounts WHERE id = ?1",
                params![id],
                map_account_row,
            )
            .optional()?)
    }

    pub fn upsert_account(&self, a: &Account) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO accounts (
                id, username, uuid, account_type, skin_variant, skin_url,
                custom_skin_path, custom_cape_path, custom_elytra_path,
                token_expires_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                username = excluded.username,
                uuid = excluded.uuid,
                account_type = excluded.account_type,
                skin_variant = excluded.skin_variant,
                skin_url = excluded.skin_url,
                custom_skin_path = excluded.custom_skin_path,
                custom_cape_path = excluded.custom_cape_path,
                custom_elytra_path = excluded.custom_elytra_path,
                token_expires_at = excluded.token_expires_at",
            params![
                a.id,
                a.username,
                a.uuid,
                a.account_type.as_str(),
                a.skin_variant.as_str(),
                a.skin_url,
                a.custom_skin_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                a.custom_cape_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                a.custom_elytra_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                a.token_expires_at,
                a.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, id: &str) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_servers(&self) -> ThkResult<Vec<ManagedServer>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, engine, game_version, server_dir, is_custom_dir,
                    port, ram_min_mb, ram_max_mb, java_path, jvm_flags, jar_file, created_at
             FROM servers ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], map_server_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_server(&self, id: &str) -> ThkResult<Option<ManagedServer>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn
            .query_row(
                "SELECT id, name, engine, game_version, server_dir, is_custom_dir,
                        port, ram_min_mb, ram_max_mb, java_path, jvm_flags, jar_file, created_at
                 FROM servers WHERE id = ?1",
                params![id],
                map_server_row,
            )
            .optional()?)
    }

    pub fn upsert_server(&self, s: &ManagedServer) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO servers (
                id, name, engine, game_version, server_dir, is_custom_dir,
                port, ram_min_mb, ram_max_mb, java_path, jvm_flags, jar_file, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                engine = excluded.engine,
                game_version = excluded.game_version,
                server_dir = excluded.server_dir,
                is_custom_dir = excluded.is_custom_dir,
                port = excluded.port,
                ram_min_mb = excluded.ram_min_mb,
                ram_max_mb = excluded.ram_max_mb,
                java_path = excluded.java_path,
                jvm_flags = excluded.jvm_flags,
                jar_file = excluded.jar_file",
            params![
                s.id,
                s.name,
                s.engine.as_str(),
                s.game_version,
                s.server_dir.to_string_lossy().as_ref(),
                s.is_custom_dir as i32,
                s.port as i64,
                s.ram_min_mb,
                s.ram_max_mb,
                s.java_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                s.jvm_flags,
                s.jar_file,
                s.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_server(&self, id: &str) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM servers WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_bookmarks(&self) -> ThkResult<Vec<ServerBookmark>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, address, created_at FROM server_bookmarks ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ServerBookmark {
                id: r.get(0)?,
                name: r.get(1)?,
                address: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn upsert_bookmark(&self, b: &ServerBookmark) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO server_bookmarks (id, name, address, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, address = excluded.address",
            params![b.id, b.name, b.address, b.created_at],
        )?;
        Ok(())
    }

    pub fn delete_bookmark(&self, id: &str) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM server_bookmarks WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_installed_content(
        &self,
        profile_id: &str,
        kind: ContentKind,
    ) -> ThkResult<Vec<InstalledContentRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT profile_id, content_kind, file_name, sha1, provider, project_id,
                    version_id, title, version_number, icon_url, enabled, installed_at
             FROM installed_content
             WHERE profile_id = ?1 AND content_kind = ?2
             ORDER BY title COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map(params![profile_id, kind.as_str()], |r| {
            let kind_str: String = r.get(1)?;
            let prov_str: String = r.get(4)?;
            let enabled_i: i32 = r.get(10)?;
            Ok(InstalledContentRecord {
                profile_id: r.get(0)?,
                content_kind: ContentKind::from_str_lossy(&kind_str),
                file_name: r.get(2)?,
                sha1: r.get(3)?,
                provider: ProviderSource::from_str_lossy(&prov_str),
                project_id: r.get(5)?,
                version_id: r.get(6)?,
                title: r.get(7)?,
                version_number: r.get(8)?,
                icon_url: r.get(9)?,
                enabled: enabled_i != 0,
                installed_at: r.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn upsert_installed_content(&self, rec: &InstalledContentRecord) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO installed_content (
                profile_id, content_kind, file_name, sha1, provider, project_id,
                version_id, title, version_number, icon_url, enabled, installed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(profile_id, content_kind, file_name) DO UPDATE SET
                sha1 = excluded.sha1,
                provider = excluded.provider,
                project_id = excluded.project_id,
                version_id = excluded.version_id,
                title = excluded.title,
                version_number = excluded.version_number,
                icon_url = excluded.icon_url,
                enabled = excluded.enabled,
                installed_at = excluded.installed_at",
            params![
                rec.profile_id,
                rec.content_kind.as_str(),
                rec.file_name,
                rec.sha1,
                rec.provider.as_str(),
                rec.project_id,
                rec.version_id,
                rec.title,
                rec.version_number,
                rec.icon_url,
                rec.enabled as i32,
                rec.installed_at,
            ],
        )?;
        Ok(())
    }

    pub fn set_installed_content_enabled(
        &self,
        profile_id: &str,
        kind: ContentKind,
        file_name: &str,
        enabled: bool,
    ) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE installed_content SET enabled = ?4 WHERE profile_id = ?1 AND content_kind = ?2 AND file_name = ?3",
            params![profile_id, kind.as_str(), file_name, enabled as i32],
        )?;
        Ok(())
    }

    pub fn remove_installed_content(
        &self,
        profile_id: &str,
        kind: ContentKind,
        file_name: &str,
    ) -> ThkResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM installed_content WHERE profile_id = ?1 AND content_kind = ?2 AND file_name = ?3",
            params![profile_id, kind.as_str(), file_name],
        )?;
        Ok(())
    }
}

fn map_profile_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GameProfile> {
    let loader_s: String = r.get(3)?;
    let inst_s: String = r.get(5)?;
    let is_custom: i32 = r.get(6)?;
    let java_s: Option<String> = r.get(8)?;
    let auto_perf: i32 = r.get(10)?;
    let play_secs: i64 = r.get(12)?;
    Ok(GameProfile {
        id: r.get(0)?,
        name: r.get(1)?,
        game_version: r.get(2)?,
        loader: LoaderType::from_str_lossy(&loader_s),
        loader_version: r.get(4)?,
        instance_path: PathBuf::from(inst_s),
        is_custom_path: is_custom != 0,
        ram_mb: r.get(7)?,
        java_path: java_s.map(PathBuf::from),
        jvm_args: r.get(9)?,
        auto_perf_mods: auto_perf != 0,
        icon_url: r.get(11)?,
        total_play_seconds: play_secs.max(0) as u64,
        session_count: r.get(13)?,
        crash_count: r.get(14)?,
        last_played_at: r.get(15)?,
        created_at: r.get(16)?,
    })
}

fn map_account_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let acc_type: String = r.get(3)?;
    let variant: String = r.get(4)?;
    let skin_p: Option<String> = r.get(6)?;
    let cape_p: Option<String> = r.get(7)?;
    let elytra_p: Option<String> = r.get(8)?;
    Ok(Account {
        id: r.get(0)?,
        username: r.get(1)?,
        uuid: r.get(2)?,
        account_type: AccountType::from_str_lossy(&acc_type),
        skin_variant: SkinVariant::from_str_lossy(&variant),
        skin_url: r.get(5)?,
        custom_skin_path: skin_p.map(PathBuf::from),
        custom_cape_path: cape_p.map(PathBuf::from),
        custom_elytra_path: elytra_p.map(PathBuf::from),
        token_expires_at: r.get(9)?,
        created_at: r.get(10)?,
    })
}

fn map_server_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ManagedServer> {
    let eng_s: String = r.get(2)?;
    let dir_s: String = r.get(4)?;
    let is_custom: i32 = r.get(5)?;
    let port_i: i64 = r.get(6)?;
    let java_s: Option<String> = r.get(9)?;
    Ok(ManagedServer {
        id: r.get(0)?,
        name: r.get(1)?,
        engine: ServerEngine::from_str_lossy(&eng_s),
        game_version: r.get(3)?,
        server_dir: PathBuf::from(dir_s),
        is_custom_dir: is_custom != 0,
        port: port_i.clamp(1, 65535) as u16,
        ram_min_mb: r.get(7)?,
        ram_max_mb: r.get(8)?,
        java_path: java_s.map(PathBuf::from),
        jvm_flags: r.get(10)?,
        jar_file: r.get(11)?,
        created_at: r.get(12)?,
    })
}
