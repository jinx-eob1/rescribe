use anyhow::Result;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db {
    db: Arc<Mutex<rusqlite::Connection>>
}

impl Db {
    pub fn open(path: &str) -> Result<Db> {
        if std::path::Path::new(&path).is_file() {
            Ok(Db { db: Arc::new(Mutex::new(rusqlite::Connection::open(path)?)) })
        }
        else {
            Self::create(path)
        }
    }
    
    fn create(path: &str) -> Result<Db> {
        let db = rusqlite::Connection::open(path)?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
                original     TEXT PRIMARY KEY,
                translations JSON
            );"
        )?;
    
        let db = Arc::new(Mutex::new(db));
        Ok(Db{db})
    }

    pub fn lookup_translation(&self, original: &str) -> Result<Option<String>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare_cached("SELECT translations FROM entries WHERE entries.original = ?")?;

        let mut rows = stmt.query(rusqlite::params![original])?;

        match rows.next()? {
            Some(row) => {
                let translation: String = row.get(0)?;
                Ok(Some(translation))
            },
            None => Ok(None)
        }
    }

    pub fn add_translation(&self, original: &str, target_lang: &str, translation: &str) -> Result<()> {
        let db = self.db.lock().unwrap();

        // The translations column looks like {"ja": "世界", es: "mundo"}
        // we will add it as new or update an existing column by adding a new translation
        let mut insert_stmt = db.prepare_cached(
            "INSERT OR REPLACE INTO
                entries(original, translations)
                VALUES(?1,
                       json_set(
                           COALESCE(
                               (SELECT
                                    translations
                                FROM
                                    entries
                                WHERE
                                    original = ?1),
                           '{}'), ?2, ?3
                       )
                )")?;

        insert_stmt.execute(rusqlite::params![original, format!("$.{target_lang}"), translation])?;

        Ok(())
    }

}
