#![allow(dead_code)]

use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::Path;

use bytes::Bytes;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::{params, Connection, DatabaseName, OpenFlags, ToSql};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioData {
    id: Uuid,
    format: String,
    bytes: Bytes,
}

impl AudioData {
    pub fn new(id: Uuid, format: String, bytes: Bytes) -> Self {
        Self { id, format, bytes }
    }
}

pub struct AudioStorage {
    conn: RefCell<Connection>,
}

impl AudioStorage {
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
            CREATE TABLE IF NOT EXISTS audio(
                id STRING PRIMARY KEY,
                format STRING NOT NULL,
                bytes BLOB NOT NULL
            )"#,
        )?;

        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    pub fn insert(&self, data: &AudioData) -> anyhow::Result<()> {
        let mut conn: std::cell::RefMut<Connection> = self.conn.borrow_mut();
        conn.transaction().and_then(|tx| {
            tx.execute(
                &format!(
                    "INSERT INTO audio VALUES(?, ?, ZEROBLOB({}))",
                    data.bytes.len()
                ),
                params![data.id.to_string(), data.format],
            )?;

            tx.blob_open(
                DatabaseName::Main,
                "audio",
                "bytes",
                tx.last_insert_rowid(),
                false,
            )?
            .write_all(data.bytes.as_ref())
            .map_err(|_| rusqlite::Error::BlobSizeError)?;

            tx.commit()
        })?;

        Ok(())
    }

    pub fn get(&self, id: Uuid) -> anyhow::Result<AudioData> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare("SELECT rowid, format FROM audio WHERE id=?")?;
        let data = stmt.query_row([id.to_string()], |row| {
            let rowid = row.get(0)?;
            let format = row.get(1)?;

            let mut blob = conn.blob_open(DatabaseName::Main, "audio", "bytes", rowid, true)?;
            let mut buffer = Vec::new();
            blob.read_to_end(&mut buffer)
                .map_err(|e| FromSqlError::Other(Box::new(e)))?;
            Ok(AudioData::new(id, format, buffer.into()))
        })?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{AudioData, AudioStorage};

    #[test]
    fn test() {
        let data = AudioData::new(
            Uuid::new_v4(),
            "audio/aac".to_owned(),
            b"1234567890".as_ref().into(),
        );

        let db = AudioStorage::new(&"./test_audio.db").unwrap();
        db.insert(&data).unwrap();

        let result = db.get(data.id).unwrap();
        assert_eq!(result, data);
    }
}
