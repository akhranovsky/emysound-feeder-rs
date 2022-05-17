#![allow(dead_code)]

use std::cell::RefCell;
use std::io::Write;

use anyhow::ensure;
use bytes::Bytes;
use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::{params, Connection, DatabaseName, OpenFlags, ToSql};
use uuid::Uuid;

use crate::{ContentType, SuggestedSegmentContentKind};

pub struct Db {
    conn: RefCell<Connection>,
}

impl Db {
    pub fn new() -> anyhow::Result<Db> {
        let mut conn = Connection::open_with_flags(
            "./data.db",
            OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        init_database(&mut conn)?;

        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    pub fn add_track(
        &self,
        id: &Uuid,
        kind: SuggestedSegmentContentKind,
        content_type: ContentType,
        artist: &str,
        title: &str,
        bytes: &Bytes,
    ) -> anyhow::Result<()> {
        ensure!(!bytes.is_empty(), "Bytes may not be empty");

        let id = id.to_string();

        let mut conn: std::cell::RefMut<Connection> = self.conn.borrow_mut();
        conn.transaction().and_then(|tx| {
            tx.prepare_cached("INSERT INTO main(id, kind, artist, title) VALUES(?, ?, ?, ?)")?
                .execute(params![id, kind.to_string(), artist, title])?;

            tx.prepare(&format!(
                "INSERT INTO data VALUES(?, ?, ZEROBLOB({}))",
                bytes.len()
            ))?
            .execute(params![id, content_type])?;

            tx.blob_open(
                DatabaseName::Main,
                "data",
                "bytes",
                tx.last_insert_rowid(),
                false,
            )?
            .write_all(bytes.as_ref())
            .map_err(|_| rusqlite::Error::BlobSizeError)?;
            tx.commit()
        })?;

        Ok(())
    }

    pub fn add_match(&self, id: &Uuid) -> anyhow::Result<()> {
        self.conn
            .borrow()
            .prepare_cached("INSERT INTO matches(id) VALUES(?)")?
            .execute([id.to_string()])?;

        Ok(())
    }
}

impl ToSql for ContentType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::from(self.to_string())))
    }
}

fn init_database(conn: &mut Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"BEGIN;
        CREATE TABLE IF NOT EXISTS main(
            id STRING PRIMARY KEY,
            added DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
            kind STRING NOT NULL,
            artist STRING NOT NULL,
            title STRING NOT NULL
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS data(
            id STRING PRIMARY KEY,
            type STRING NOT NULL,
            bytes BLOB NOT NULL,
            FOREIGN KEY (id)
            REFERENCES main (id)
                ON DELETE CASCADE
                ON UPDATE NO ACTION
        );
        CREATE TABLE IF NOT EXISTS matches(
            id STRING PRIMARY KEY,
            matched DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
            FOREIGN KEY (id)
            REFERENCES main (id)
                ON DELETE CASCADE
                ON UPDATE NO ACTION
        )
        WITHOUT ROWID;
        COMMIT;"#,
    )
    .map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use uuid::{uuid, Uuid};

    use crate::{ContentType, SuggestedSegmentContentKind};

    use super::Db;

    static UUID: Uuid = uuid!("5e9805d9-276e-42d3-9736-637e64a78f98");

    #[test]
    fn test_a_add_track() {
        let db = Db::new().unwrap();

        db.add_track(
            &UUID,
            SuggestedSegmentContentKind::Music,
            ContentType::Aac,
            "Artist 1",
            "title 1",
            &Bytes::from_static(b"1234567890"),
        )
        .unwrap();
    }

    #[test]
    fn test_b_add_match() {
        let db = Db::new().unwrap();

        db.add_match(&UUID).unwrap();
    }
}
