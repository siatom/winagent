use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_titles_apply_once_and_never_override_manual_names_or_deleted_sessions() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let session = db.create_session("新会话", None, None).unwrap();
        assert!(db.needs_auto_title(&session.id).unwrap());
        assert!(db.set_auto_title(&session.id, "temporary", false).unwrap());
        assert!(db.needs_auto_title(&session.id).unwrap());
        assert!(db.set_auto_title(&session.id, "generated", true).unwrap());
        assert!(!db.needs_auto_title(&session.id).unwrap());
        assert!(!db.set_auto_title(&session.id, "late result", true).unwrap());
        let manual = db.create_session("新会话", None, None).unwrap();
        db.rename_session(&manual.id, "my title").unwrap();
        assert!(!db.set_auto_title(&manual.id, "generated", true).unwrap());
        assert_eq!(
            db.get_session(&manual.id).unwrap().unwrap().title,
            "my title"
        );
        db.delete_session(&session.id).unwrap();
        assert!(!db.set_auto_title(&session.id, "generated", true).unwrap());
    }

    #[test]
    fn migrates_legacy_titles_without_enabling_automatic_rename() {
        let path =
            std::env::temp_dir().join(format!("winagent-titles-{}.db", uuid::Uuid::new_v4()));
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT NOT NULL, provider_id TEXT, model TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
                INSERT INTO sessions VALUES ('legacy', 'My old title', NULL, NULL, 1, 1);").unwrap();
            conn.execute_batch("CREATE TABLE messages (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL DEFAULT '', reasoning TEXT, tool_calls TEXT, tool_call_id TEXT, name TEXT, meta TEXT, created_at INTEGER NOT NULL);
                INSERT INTO messages (id, session_id, role, content, created_at) VALUES ('msg', 'legacy', 'user', 'hello', 1);").unwrap();
        }
        for _ in 0..2 {
            let db = Db::open(&path).unwrap();
            assert_eq!(
                db.get_session("legacy").unwrap().unwrap().title,
                "My old title"
            );
            assert!(!db.needs_auto_title("legacy").unwrap());
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn removes_only_sessions_without_messages() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let empty = db.create_session("empty", None, None).unwrap();
        let saved = db.create_session("saved", None, None).unwrap();
        db.insert_message(
            "message", &saved.id, "user", "hello", None, None, None, None, None,
        )
        .unwrap();
        assert_eq!(db.delete_empty_sessions().unwrap(), 1);
        assert!(db.get_session(&empty.id).unwrap().is_none());
        assert!(db.get_session(&saved.id).unwrap().is_some());
        assert_eq!(db.list_messages(&saved.id).unwrap()[0].content, "hello");
        assert_eq!(db.delete_empty_sessions().unwrap(), 0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    pub meta: Option<Value>,
    pub created_at: i64,
}

pub struct Db {
    conn: Mutex<Connection>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                provider_id TEXT,
                model TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                reasoning TEXT,
                tool_calls TEXT,
                tool_call_id TEXT,
                name TEXT,
                meta TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);
            CREATE TABLE IF NOT EXISTS providers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                base_url TEXT NOT NULL,
                models TEXT NOT NULL DEFAULT '[]',
                default_model TEXT NOT NULL,
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tool_runs (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                tool TEXT NOT NULL,
                command TEXT NOT NULL,
                risk_level TEXT NOT NULL,
                decision TEXT NOT NULL,
                exit_code INTEGER,
                ok INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                output TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        let has_title_mode = {
            let mut stmt = conn.prepare("PRAGMA table_info(sessions)")?;
            let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
            columns
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|name| name == "title_mode")
        };
        if !has_title_mode {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN title_mode TEXT NOT NULL DEFAULT 'manual'",
                [],
            )?;
        }
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.seed_defaults()?;
        db.delete_empty_sessions()?;
        Ok(db)
    }

    fn seed_defaults(&self) -> rusqlite::Result<()> {
        let count: i64 =
            self.conn
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))?;
        if count == 0 {
            self.conn.lock().unwrap().execute(
                "INSERT INTO providers (id, name, base_url, models, default_model, is_default, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
                params![
                    "deepseek",
                    "DeepSeek",
                    "https://api.deepseek.com",
                    r#"["deepseek-flash","deepseek-chat","deepseek-reasoner"]"#,
                    "deepseek-flash",
                    now_ms()
                ],
            )?;
        }
        Ok(())
    }

    // ---------- sessions ----------

    pub fn delete_empty_sessions(&self) -> rusqlite::Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM tool_runs WHERE session_id IN (SELECT id FROM sessions WHERE NOT EXISTS (SELECT 1 FROM messages WHERE messages.session_id = sessions.id))",
            [],
        )?;
        let deleted = tx.execute(
            "DELETE FROM sessions WHERE NOT EXISTS (SELECT 1 FROM messages WHERE messages.session_id = sessions.id)",
            [],
        )?;
        tx.commit()?;
        Ok(deleted)
    }

    pub fn list_sessions(&self) -> rusqlite::Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, provider_id, model, created_at, updated_at FROM sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Session {
                id: r.get(0)?,
                title: r.get(1)?,
                provider_id: r.get(2)?,
                model: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_session(&self, id: &str) -> rusqlite::Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, title, provider_id, model, created_at, updated_at FROM sessions WHERE id = ?1",
            [id],
            |r| {
                Ok(Session {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    provider_id: r.get(2)?,
                    model: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            },
        )
        .optional()
    }

    pub fn create_session(
        &self,
        title: &str,
        provider_id: Option<&str>,
        model: Option<&str>,
    ) -> rusqlite::Result<Session> {
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            provider_id: provider_id.map(str::to_string),
            model: model.map(str::to_string),
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO sessions (id, title, provider_id, model, created_at, updated_at, title_mode) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending')",
            params![session.id, session.title, session.provider_id, session.model, session.created_at, session.updated_at],
        )?;
        Ok(session)
    }

    pub fn import_session(
        &self,
        id: &str,
        title: &str,
        created_at: i64,
        updated_at: i64,
    ) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO sessions (id, title, provider_id, model, created_at, updated_at, title_mode)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, 'manual')",
            params![id, title, created_at, updated_at],
        )?;
        Ok(())
    }

    pub fn rename_session(&self, id: &str, title: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET title = ?2, title_mode = 'manual', updated_at = ?3 WHERE id = ?1",
            params![id, title, now_ms()],
        )?;
        Ok(())
    }

    pub fn needs_auto_title(&self, id: &str) -> rusqlite::Result<bool> {
        self.conn.lock().unwrap().query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1 AND title_mode = 'pending')",
            [id],
            |row| row.get(0),
        )
    }

    pub fn set_auto_title(
        &self,
        id: &str,
        title: &str,
        final_title: bool,
    ) -> rusqlite::Result<bool> {
        let changed = self.conn.lock().unwrap().execute(
            "UPDATE sessions SET title = ?2, title_mode = ?3 WHERE id = ?1 AND title_mode = 'pending'",
            params![id, title, if final_title { "auto" } else { "pending" }],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_session(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM messages WHERE session_id = ?1", [id])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_session_model(
        &self,
        id: &str,
        provider_id: Option<&str>,
        model: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET provider_id = ?2, model = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, provider_id, model, now_ms()],
        )?;
        Ok(())
    }

    pub fn touch_session(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        Ok(())
    }

    // ---------- messages ----------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_message(
        &self,
        id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        reasoning: Option<&str>,
        tool_calls: Option<&str>,
        tool_call_id: Option<&str>,
        name: Option<&str>,
        meta: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.insert_message_at(
            id,
            session_id,
            role,
            content,
            reasoning,
            tool_calls,
            tool_call_id,
            name,
            meta,
            now_ms(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_message_at(
        &self,
        id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        reasoning: Option<&str>,
        tool_calls: Option<&str>,
        tool_call_id: Option<&str>,
        name: Option<&str>,
        meta: Option<&str>,
        created_at: i64,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO messages (id, session_id, role, content, reasoning, tool_calls, tool_call_id, name, meta, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, session_id, role, content, reasoning, tool_calls, tool_call_id, name, meta, created_at],
        )?;
        conn.execute(
            "UPDATE sessions SET updated_at = MAX(updated_at, ?2) WHERE id = ?1",
            params![session_id, created_at],
        )?;
        Ok(())
    }

    pub fn list_messages(&self, session_id: &str) -> rusqlite::Result<Vec<MessageDto>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, reasoning, tool_calls, tool_call_id, name, meta, created_at
             FROM messages WHERE session_id = ?1 ORDER BY created_at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([session_id], |r| {
            let tool_calls: Option<String> = r.get(5)?;
            let meta: Option<String> = r.get(8)?;
            Ok(MessageDto {
                id: r.get(0)?,
                session_id: r.get(1)?,
                role: r.get(2)?,
                content: r.get(3)?,
                reasoning: r.get(4)?,
                tool_calls: tool_calls.and_then(|s| serde_json::from_str(&s).ok()),
                tool_call_id: r.get(6)?,
                name: r.get(7)?,
                meta: meta.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: r.get(9)?,
            })
        })?;
        rows.collect()
    }

    pub fn message_count(&self, session_id: &str) -> rusqlite::Result<i64> {
        self.conn.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            [session_id],
            |r| r.get(0),
        )
    }

    // ---------- providers ----------

    pub fn list_providers(&self) -> rusqlite::Result<Vec<Provider>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, models, default_model, is_default FROM providers ORDER BY is_default DESC, created_at ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            let models: String = r.get(3)?;
            Ok(Provider {
                id: r.get(0)?,
                name: r.get(1)?,
                base_url: r.get(2)?,
                models: serde_json::from_str(&models).unwrap_or_default(),
                default_model: r.get(4)?,
                is_default: r.get::<_, i64>(5)? == 1,
            })
        })?;
        rows.collect()
    }

    pub fn get_provider(&self, id: &str) -> rusqlite::Result<Option<Provider>> {
        Ok(self.list_providers()?.into_iter().find(|p| p.id == id))
    }

    pub fn default_provider(&self) -> rusqlite::Result<Option<Provider>> {
        Ok(self
            .list_providers()?
            .into_iter()
            .find(|p| p.is_default)
            .or_else(|| self.list_providers().ok()?.into_iter().next()))
    }

    pub fn upsert_provider(&self, provider: &Provider) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        if provider.is_default {
            conn.execute("UPDATE providers SET is_default = 0", [])?;
        }
        conn.execute(
            "INSERT INTO providers (id, name, base_url, models, default_model, is_default, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                base_url = excluded.base_url,
                models = excluded.models,
                default_model = excluded.default_model,
                is_default = excluded.is_default",
            params![
                provider.id,
                provider.name,
                provider.base_url,
                serde_json::to_string(&provider.models).unwrap_or_else(|_| "[]".into()),
                provider.default_model,
                if provider.is_default { 1 } else { 0 },
                now_ms()
            ],
        )?;
        Ok(())
    }

    pub fn set_default_provider(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE providers SET is_default = 0", [])?;
        conn.execute("UPDATE providers SET is_default = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn delete_provider(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM providers WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- settings ----------

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------- audit ----------

    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_run(
        &self,
        session_id: &str,
        tool: &str,
        command: &str,
        risk_level: &str,
        decision: &str,
        exit_code: Option<i32>,
        ok: bool,
        duration_ms: u64,
        output: &str,
    ) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO tool_runs (id, session_id, tool, command, risk_level, decision, exit_code, ok, duration_ms, output, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                uuid::Uuid::new_v4().to_string(),
                session_id,
                tool,
                command,
                risk_level,
                decision,
                exit_code,
                if ok { 1 } else { 0 },
                duration_ms as i64,
                output,
                now_ms()
            ],
        )?;
        Ok(())
    }
}
