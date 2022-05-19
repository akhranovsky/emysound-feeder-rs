#![allow(dead_code)]

use std::cell::RefCell;
use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OpenFlags};
use uuid::Uuid;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MatchData {
    id: Uuid,
    timestamp: DateTime<Utc>,
    score: u8,
}

impl MatchData {
    pub fn new(id: Uuid, timestamp: DateTime<Utc>, score: u8) -> Self {
        Self {
            id,
            timestamp,
            score,
        }
    }
}

pub struct MatchesStorage {
    conn: RefCell<Connection>,
}

impl MatchesStorage {
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
            CREATE TABLE IF NOT EXISTS matches(
                id STRING NOT NULL,
                timestamp DATETIME NOT NULL,
                score INTEGER NOT NULL
            )"#,
        )?;

        Ok(Self {
            conn: RefCell::new(conn),
        })
    }

    fn insert(&self, data: &MatchData) -> anyhow::Result<()> {
        let conn = self.conn.borrow_mut();
        conn.prepare_cached("INSERT INTO matches VALUES(?, ?, ?)")
            .context("Prepare statement")?
            .execute(params![data.id.to_string(), data.timestamp, data.score])
            .context("Execute statement")?;
        Ok(())
    }

    fn get(&self, id: Uuid) -> anyhow::Result<Vec<MatchData>> {
        let conn = self.conn.borrow();
        let mut stmt = conn
            .prepare("SELECT timestamp, score FROM matches WHERE id=? ORDER BY timestamp DESC")?;
        let rows = stmt.query([id.to_string()])?;
        rows.mapped(|row| {
            let timestamp: DateTime<Utc> = row.get(0)?;
            let score: u8 = row.get(1)?;
            Ok(MatchData::new(id, timestamp, score))
        })
        .map(|m| m.map_err(|e| e.into()))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use crate::storage::matches::{MatchData, MatchesStorage};

    #[test]
    fn test() {
        let id = Uuid::new_v4();
        let data1 = MatchData::new(id, Utc::now(), 25);
        let data2 = MatchData::new(id, Utc::now() - chrono::Duration::seconds(1), 95);

        let db = MatchesStorage::new(&"./test_matches.db").unwrap();
        db.insert(&data1).unwrap();
        db.insert(&data2).unwrap();

        let result = db.get(id).unwrap();
        assert_eq!(&result, &[data1, data2]);
    }
}
