use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;
use std::mem::transmute;
use std::sync::Once;

static INIT: Once = Once::new();

/// Initializes the sqlite-vec extension for vector similarity search.
#[expect(
    clippy::missing_transmute_annotations,
    reason = "transmute required for sqlite extension registration FFI"
)]
#[expect(unsafe_code, reason = "FFI call to sqlite3_auto_extension requires unsafe")]
#[expect(clippy::as_conversions, reason = "required for sqlite extension FFI")]
pub fn init_sqlite_vec() {
    INIT.call_once(|| {
        #[expect(clippy::fn_to_numeric_cast_any, reason = "required for sqlite extension FFI")]
        let init_fn = sqlite3_vec_init as *const ();
        let transmuted = unsafe { transmute(init_fn) };
        unsafe { sqlite3_auto_extension(Some(transmuted)) };
        tracing::info!("sqlite-vec extension registered");
    });
}
