#![allow(dead_code)]

use std::cell::RefCell;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::{params, Connection, OpenFlags, ToSql};
use uuid::Uuid;

pub struct MetadataStorage {
    conn: RefCell<Connection>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AudioKind {
    Unknown,
    Audio,
    Advertisement,
    Music,
    Talk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    id: Uuid,
    date: DateTime<Utc>,
    kind: AudioKind,
    artist: String,
    title: String,
}

impl Metadata {
    pub fn new(
        id: Uuid,
        date: DateTime<Utc>,
        kind: AudioKind,
        artist: String,
        title: String,
    ) -> Self {
        Self {
            id,
            date,
            kind,
            artist,
            title,
        }
    }
}

impl ToSql for AudioKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            AudioKind::Unknown => "unknown".to_sql(),
            AudioKind::Audio => "audio".to_sql(),
            AudioKind::Advertisement => "advertisement".to_sql(),
            AudioKind::Music => "music".to_sql(),
            AudioKind::Talk => "talk".to_sql(),
        }
    }
}

impl FromSql for AudioKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_str().and_then(|v| match v {
            "audio" => Ok(AudioKind::Audio),
            "advertisement" => Ok(AudioKind::Advertisement),
            "music" => Ok(AudioKind::Music),
            "talk" => Ok(AudioKind::Talk),
            "unknown" => Ok(AudioKind::Unknown),
            _ => Err(FromSqlError::InvalidType),
        })
    }
}

impl MetadataStorage {
    pub fn new<P>(path: &P) -> anyhow::Result<Self>
    where
        P: AsRef<Path>,
    {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;

        conn.execute_batch(
            r#"
        CREATE TABLE IF NOT EXISTS metadata(
            id STRING PRIMARY KEY,
            date DATETIME NOT NULL,
            kind STRING NOT NULL,
            artist STRING NOT NULL,
            title STRING NOT NULL
        ) WITHOUT ROWID"#,
        )?;

        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    pub fn insert(&self, metadata: &Metadata) -> anyhow::Result<()> {
        self.conn
            .borrow_mut()
            .prepare_cached(
                "INSERT INTO metadata(id, date, kind, artist, title) VALUES(?, ?, ?, ?, ?)",
            )?
            .execute(params![
                metadata.id.to_string(),
                metadata.date,
                metadata.kind,
                metadata.artist,
                metadata.title
            ])?;

        Ok(())
    }

    pub fn get(&self, id: Uuid) -> anyhow::Result<Metadata> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare("SELECT date, kind, artist, title FROM metadata WHERE id=?")?;
        let data = stmt.query_row([id.to_string()], |row| {
            let date: DateTime<Utc> = row.get(0)?;
            let kind: AudioKind = row.get(1)?;
            let artist = row.get(2)?;
            let title = row.get(3)?;
            Ok(Metadata::new(id, date, kind, artist, title))
        })?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::{Metadata, MetadataStorage};

    #[test]
    fn test_existing() {
        let metadata = Metadata::new(
            Uuid::new_v4(),
            Utc::now(),
            super::AudioKind::Music,
            "Artist".to_string(),
            "Title".to_string(),
        );

        let storage = MetadataStorage::new(&"./test_metadata.db").unwrap();
        storage.insert(&metadata).unwrap();
        let result = storage.get(metadata.id).unwrap();

        assert_eq!(metadata, result);
    }

    #[test]
    fn test_non_existing() {
        let storage = MetadataStorage::new(&"./test_metadata.db").unwrap();
        assert!(storage.get(Uuid::new_v4()).is_err());
    }
}
