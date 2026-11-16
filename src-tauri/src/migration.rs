use std::path::Path;

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
