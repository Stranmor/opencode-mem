//! SQLite storage implementation

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::migrations;

pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    pub fn new(db_path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        let conn = storage.conn.lock().unwrap();
        migrations::run_migrations(&conn)?;
        drop(conn);

        Ok(storage)
    }

    // TODO: Implement CRUD operations
    // Port from existing storage.rs
}
