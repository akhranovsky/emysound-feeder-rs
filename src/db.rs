#![allow(dead_code)]

use std::cell::RefCell;

use anyhow::ensure;
use bytes::Bytes;
use chrono::Utc;
use rusqlite::{params, Connection, OpenFlags};
use uuid::Uuid;

use crate::SuggestedSegmentContentKind;

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

    pub fn add(
        &self,
        id: &Uuid,
        kind: SuggestedSegmentContentKind,
        artist: &str,
        title: &str,
        bytes: &Bytes,
    ) -> anyhow::Result<()> {
        ensure!(!bytes.is_empty(), "Bytes may not be empty");

        let id = id.to_string();
        let conn = self.conn.borrow();
        let mut conn_mut = self.conn.borrow_mut();

        let tx = conn_mut.transaction()?;

        conn.prepare_cached("INSERT INTO main VALUES(?, ?, ?, ?, ?)")?
            .execute(params![id, Utc::now(), kind.to_string(), artist, title])?;
        conn.prepare_cached("INSERT INTO data VALUES(?, ?, ?)")?
            .execute(params![id, "aac", bytes.to_vec()])?;

        tx.commit()?;

        Ok(())
    }
}

fn init_database(conn: &mut Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"BEGIN;
        CREATE TABLE IF NOT EXISTS main(
            id STRING PRIMARY KEY,
            data DATETIME NOT NULL,
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
        )
        WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS matches(
            id STRING PRIMARY KEY,
            date datetime NOT NULL,
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
    use super::Db;

    #[test]
    fn test_init_db() {
        Db::new().unwrap();
    }
}
