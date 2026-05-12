use anyhow::Result;
use rusqlite::params;
use crate::infrastructure::database::SqliteDb;
use super::DbOperations;

impl DbOperations {
    pub fn get_source_dir(db: &SqliteDb) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        let conn = db.conn();
        let dir = conn.query_row(
            "SELECT source_dir FROM config_auraseek WHERE id = 'main'",
            [],
            |r| r.get::<_, String>(0),
        ).optional()?;
        Ok(dir)
    }

    pub fn set_source_dir(db: &SqliteDb, source_dir: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute(
            "INSERT OR REPLACE INTO config_auraseek (id, source_dir, updated_at)
             VALUES ('main', ?1, datetime('now'))",
            params![source_dir],
        )?;
        Ok(())
    }
}
