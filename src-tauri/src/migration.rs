use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::db::Db;

/// Preserve the source and include committed WAL data in the imported snapshot.
pub fn import_database(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() || !source.is_file() {
        return Ok(());
    }
    let staging = destination.with_file_name(format!("migration-{}.db", uuid::Uuid::new_v4()));
    let result = (|| {
        let connection = rusqlite::Connection::open_with_flags(
            source,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| format!("无法读取旧版数据库: {e}"))?;
        connection
            .execute("VACUUM INTO ?1", [staging.to_string_lossy().as_ref()])
            .map_err(|e| format!("无法迁移旧版数据库: {e}"))?;
        // Publish only a completed snapshot, so an interrupted import is retried.
        std::fs::rename(&staging, destination).map_err(|e| format!("无法保存迁移后的数据库: {e}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
}

/// Import the local chat histories produced by the supported coding agents.
/// The setting makes the migration idempotent and keeps source databases untouched.
pub fn import_histories(db: &Db, app_data_parent: &Path) -> Result<(), String> {
    if db.get_setting("history_import_v1").is_some() {
        return Ok(());
    }

    let home = PathBuf::from(
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .unwrap_or_default(),
    );
    let mut imported = 0usize;
    imported += import_codex(db, &home.join(r".codex\sessions"))?;
    imported += import_codebuddy(db, &home.join(r".codebuddy\history.jsonl"))?;
    imported += import_opencode(db, &home.join(r".local\share\opencode\opencode.db"))?;
    imported += import_opencode(db, &home.join(r".local\share\gencode\opencode.db"))?;
    imported += import_qoder(
        db,
        &app_data_parent.join(r"com.qoder.app.stable\main.sqlite"),
    )?;
    imported += import_qoder(
        db,
        &app_data_parent.join(r"com.qodercn.app.stable\main.sqlite"),
    )?;
    db.set_setting("history_import_v1", &imported.to_string())
        .map_err(|e| format!("无法记录历史迁移状态: {e}"))?;
    Ok(())
}

fn import_codex(db: &Db, root: &Path) -> Result<usize, String> {
    if !root.is_dir() {
        return Ok(0);
    }
    let mut count = 0;
    for file in walk_jsonl(root) {
        let Ok(lines) = File::open(&file).map(BufReader::new) else {
            continue;
        };
        let mut session_id = None;
        let mut title = None;
        let mut created = i64::MAX;
        let mut updated = 0i64;
        let mut records = Vec::new();
        for line in lines.lines().flatten() {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let ts = value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_time);
            if let Some(ts) = ts {
                created = created.min(ts);
                updated = updated.max(ts);
            }
            if value.get("type").and_then(Value::as_str) == Some("session_meta") {
                session_id = value
                    .pointer("/payload/session_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            if value.pointer("/payload/type").and_then(Value::as_str) == Some("message") {
                let role = value
                    .pointer("/payload/role")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if matches!(role, "user" | "assistant") {
                    let content =
                        text_from_value(value.pointer("/payload/content").unwrap_or(&Value::Null));
                    if !content.trim().is_empty() {
                        records.push((role.to_string(), content, ts.unwrap_or(updated)));
                    }
                }
            }
        }
        let Some(source_id) = session_id else {
            continue;
        };
        if records.is_empty() {
            continue;
        }
        let id = format!("import:codex:{source_id}");
        let session_title = title
            .take()
            .unwrap_or_else(|| format!("Codex · {}", first_title(&records)));
        db.import_session(
            &id,
            &session_title,
            valid_time(created, updated),
            updated.max(created),
        )
        .map_err(db_err)?;
        for (index, (role, content, time)) in records.into_iter().enumerate() {
            db.insert_message_at(
                &format!("{id}:message:{index}"),
                &id,
                &role,
                &content,
                None,
                None,
                None,
                None,
                Some(r#"{"source":"codex"}"#),
                time,
            )
            .map_err(db_err)?;
            count += 1;
        }
    }
    Ok(count)
}

fn import_codebuddy(db: &Db, path: &Path) -> Result<usize, String> {
    if !path.is_file() {
        return Ok(0);
    }
    let mut grouped: BTreeMap<String, Vec<(i64, String)>> = BTreeMap::new();
    for line in BufReader::new(File::open(path).map_err(db_err)?)
        .lines()
        .flatten()
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(display) = value
            .get("display")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        else {
            continue;
        };
        let project = value
            .get("project")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let time = value.get("timestamp").and_then(Value::as_i64).unwrap_or(0);
        grouped
            .entry(project)
            .or_default()
            .push((time, display.to_string()));
    }
    let mut count = 0;
    for (project, records) in grouped {
        let key = stable_key(&project);
        let id = format!("import:codebuddy:{key}");
        let title = format!(
            "CodeBuddy · {}",
            project.rsplit(['/', '\\']).next().unwrap_or(&project)
        );
        let first = records.first().map(|r| r.0).unwrap_or(0);
        let last = records.last().map(|r| r.0).unwrap_or(first);
        db.import_session(&id, &title, first, last)
            .map_err(db_err)?;
        for (index, (time, content)) in records.into_iter().enumerate() {
            db.insert_message_at(
                &format!("{id}:message:{index}"),
                &id,
                "user",
                &content,
                None,
                None,
                None,
                None,
                Some(r#"{"source":"codebuddy"}"#),
                time,
            )
            .map_err(db_err)?;
            count += 1;
        }
    }
    Ok(count)
}

fn import_opencode(db: &Db, path: &Path) -> Result<usize, String> {
    if !path.is_file() {
        return Ok(0);
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(db_err)?;
    let mut sessions = conn.prepare("SELECT id,title,time_created,time_updated FROM session WHERE time_archived IS NULL OR time_archived = 0").map_err(db_err)?;
    let rows = sessions
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(db_err)?;
    let mut count = 0;
    for row in rows.flatten() {
        let (source_id, title, created, updated) = row;
        let id = format!("import:opencode:{source_id}");
        let directory: String = conn
            .query_row(
                "SELECT directory FROM session WHERE id = ?1",
                [&source_id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        db.import_session(&id, &format!("OpenCode · {title}"), created, updated)
            .map_err(db_err)?;
        let mut messages = conn.prepare("SELECT id,data,time_created FROM message WHERE session_id = ?1 ORDER BY time_created,id").map_err(db_err)?;
        let message_rows = messages
            .query_map([&source_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(db_err)?;
        for (index, message) in message_rows.flatten().enumerate() {
            let (message_id, data, time) = message;
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            let role = value.get("role").and_then(Value::as_str).unwrap_or("");
            if !matches!(role, "user" | "assistant") {
                continue;
            }
            let mut parts = conn
                .prepare("SELECT data FROM part WHERE message_id = ?1 ORDER BY time_created,id")
                .map_err(db_err)?;
            let part_rows = parts
                .query_map([&message_id], |r| r.get::<_, String>(0))
                .map_err(db_err)?;
            let content = part_rows
                .flatten()
                .filter_map(|part| serde_json::from_str::<Value>(&part).ok())
                .map(|part| text_from_value(&part))
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if content.is_empty() {
                continue;
            }
            let meta = serde_json::json!({"source":"opencode","directory":directory});
            db.insert_message_at(
                &format!("{id}:message:{index}"),
                &id,
                role,
                &content,
                None,
                None,
                None,
                None,
                Some(&meta.to_string()),
                time,
            )
            .map_err(db_err)?;
            count += 1;
        }
    }
    Ok(count)
}

fn import_qoder(db: &Db, path: &Path) -> Result<usize, String> {
    if !path.is_file() {
        return Ok(0);
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(db_err)?;
    let mut sessions = conn.prepare("SELECT session_id,title,created_at,updated_at FROM chat_sessions WHERE deleted_at IS NULL").map_err(db_err)?;
    let rows = sessions
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(db_err)?;
    let mut count = 0;
    for row in rows.flatten() {
        let (source_id, title, created, updated) = row;
        let id = format!("import:qoder:{source_id}");
        db.import_session(&id, &format!("Qoder · {title}"), created, updated)
            .map_err(db_err)?;
        let mut messages = conn.prepare("SELECT message_id,role,payload_json,created_at FROM chat_session_messages WHERE session_id = ?1 ORDER BY sequence,message_id").map_err(db_err)?;
        let message_rows = messages
            .query_map([&source_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(db_err)?;
        for (index, (message_id, role, payload, time)) in message_rows.flatten().enumerate() {
            if !matches!(role.as_str(), "user" | "assistant") {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                continue;
            };
            let content = text_from_value(&value);
            if content.is_empty() {
                continue;
            }
            let meta = serde_json::json!({"source":"qoder","sourceMessageId":message_id});
            db.insert_message_at(
                &format!("{id}:message:{index}"),
                &id,
                &role,
                &content,
                None,
                None,
                None,
                None,
                Some(&meta.to_string()),
                time,
            )
            .map_err(db_err)?;
            count += 1;
        }
    }
    Ok(count)
}

fn text_from_value(value: &Value) -> String {
    match value {
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                map.values()
                    .map(text_from_value)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
        Value::Array(items) => items
            .iter()
            .map(text_from_value)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn first_title(records: &[(String, String, i64)]) -> String {
    records
        .iter()
        .find(|r| r.0 == "user")
        .map(|r| r.1.chars().take(32).collect())
        .unwrap_or_else(|| "历史会话".into())
}
fn parse_time(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.timestamp_millis())
}
fn valid_time(created: i64, updated: i64) -> i64 {
    if created == i64::MAX {
        updated
    } else {
        created
    }
}
fn stable_key(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
fn db_err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn walk_jsonl(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_jsonl(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_wal_data_without_overwriting_existing_database() {
        let dir = std::env::temp_dir().join(format!("winagent-migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.db");
        let destination = dir.join("winagent.db");
        let connection = rusqlite::Connection::open(&source).unwrap();
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE history (message TEXT); INSERT INTO history VALUES ('saved conversation');").unwrap();
        import_database(&source, &destination).unwrap();
        let imported = rusqlite::Connection::open(&destination).unwrap();
        let message: String = imported
            .query_row("SELECT message FROM history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(message, "saved conversation");
        connection
            .execute("INSERT INTO history VALUES ('later')", [])
            .unwrap();
        import_database(&source, &destination).unwrap();
        let count: i64 = imported
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(source.is_file());
        drop(imported);
        drop(connection);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_source_does_not_create_database() {
        let dir = std::env::temp_dir().join(format!("winagent-migration-{}", uuid::Uuid::new_v4()));
        let destination = dir.join("winagent.db");
        import_database(&dir.join("missing.db"), &destination).unwrap();
        assert!(!destination.exists());
    }
}
