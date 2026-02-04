//! SQLite-vec extension initialization

use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;
use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize sqlite-vec extension. Must be called BEFORE opening any connection.
/// Safe to call multiple times - only initializes once.
#[allow(clippy::missing_transmute_annotations)]
pub fn init_sqlite_vec() {
    INIT.call_once(|| {
        unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }
        tracing::info!("sqlite-vec extension registered");
    });
}
