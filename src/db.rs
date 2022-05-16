#![allow(dead_code)]

use std::cell::Cell;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::{ConnectOptions, SqliteConnection};

struct Db {
    pub conn: Cell<SqliteConnection>,
}

impl Db {
    async fn new() -> anyhow::Result<Db> {
        let mut conn = SqliteConnectOptions::from_str("sqlite://data.db")?
            .journal_mode(SqliteJournalMode::Off)
            .create_if_missing(true)
            .connect()
            .await?;

        init_database(&mut conn).await?;

        Ok(Self {
            conn: Cell::new(conn),
        })
    }
}

async fn init_database(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query!(
        r#"CREATE TABLE IF NOT EXISTS main(
            id STRING PRIMARY KEY,
            data DATETIME NOT NULL,
            type STRING NOT NULL,
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
        "#
    )
    .execute(conn)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Db;

    #[tokio::test]
    async fn test_init_db() {
        let db = Db::new().await.unwrap();
    }
}
