use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Clone)]
pub struct Database {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct Transcription {
    pub id: i64,
    pub text: String,
    pub language: String,
    pub duration_ms: i64,
    pub speech_duration_ms: i64,
    pub word_count: i64,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DictationStats {
    pub total_words: i64,
    pub total_duration_ms: i64,
    pub session_count: i64,
    pub avg_wpm: f64,
}

impl Database {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        let db = Self { path };
        db.init()?;
        Ok(db)
    }

    pub fn insert(
        &self,
        text: &str,
        language: &str,
        duration_ms: i64,
        speech_duration_ms: i64,
        model: &str,
    ) -> anyhow::Result<()> {
        let word_count = text.split_whitespace().count() as i64;
        let conn = self.open()?;
        conn.execute(
            "INSERT INTO transcriptions (text, language, duration_ms, speech_duration_ms, word_count, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                text,
                language,
                duration_ms,
                speech_duration_ms,
                word_count,
                model,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        conn.execute(
            "DELETE FROM transcriptions
             WHERE id NOT IN (
               SELECT id FROM transcriptions ORDER BY id DESC LIMIT 100
             )",
            [],
        )?;
        Ok(())
    }

    pub fn list(&self, limit: i64) -> anyhow::Result<Vec<Transcription>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, text, language, duration_ms, COALESCE(speech_duration_ms, 0),
                    COALESCE(word_count, 0), model, created_at
             FROM transcriptions
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(Transcription {
                id: row.get(0)?,
                text: row.get(1)?,
                language: row.get(2)?,
                duration_ms: row.get(3)?,
                speech_duration_ms: row.get(4)?,
                word_count: row.get(5)?,
                model: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn stats_30d(&self) -> anyhow::Result<DictationStats> {
        let conn = self.open()?;
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT COALESCE(SUM(COALESCE(word_count, 0)), 0),
                    COALESCE(SUM(duration_ms), 0),
                    COALESCE(SUM(COALESCE(speech_duration_ms, 0)), 0),
                    COUNT(*)
             FROM transcriptions
             WHERE created_at >= ?1",
        )?;
        let (total_words, total_duration_ms, total_speech_ms, session_count): (i64, i64, i64, i64) =
            stmt.query_row(params![cutoff], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;

        let avg_wpm = if total_speech_ms > 0 {
            (total_words as f64 / total_speech_ms as f64) * 60_000.0
        } else {
            0.0
        };

        Ok(DictationStats {
            total_words,
            total_duration_ms,
            session_count,
            avg_wpm,
        })
    }

    pub fn delete(&self, id: i64) -> anyhow::Result<()> {
        self.open()?
            .execute("DELETE FROM transcriptions WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        self.open()?.execute("DELETE FROM transcriptions", [])?;
        Ok(())
    }

    fn init(&self) -> anyhow::Result<()> {
        let conn = self.open()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS transcriptions (
                id INTEGER PRIMARY KEY,
                text TEXT NOT NULL,
                language TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                speech_duration_ms INTEGER NOT NULL DEFAULT 0,
                word_count INTEGER NOT NULL DEFAULT 0,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_transcriptions_created
                ON transcriptions(created_at DESC);",
        )?;
        // migrate: add columns to existing tables that lack them
        if conn
            .prepare("SELECT word_count FROM transcriptions LIMIT 0")
            .is_err()
        {
            conn.execute_batch(
                "ALTER TABLE transcriptions ADD COLUMN word_count INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if conn
            .prepare("SELECT speech_duration_ms FROM transcriptions LIMIT 0")
            .is_err()
        {
            conn.execute_batch(
                "ALTER TABLE transcriptions ADD COLUMN speech_duration_ms INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        Ok(())
    }

    fn open(&self) -> anyhow::Result<Connection> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Connection::open(&self.path)?)
    }
}
