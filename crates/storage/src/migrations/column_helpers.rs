use rusqlite::Connection;

pub(super) fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("PRAGMA table_info({table})");
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return false;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    for name in rows.flatten() {
        if name == column {
            return true;
        }
    }
    false
}

pub(super) fn add_column_if_not_exists(
    conn: &Connection,
    table: &str,
    column: &str,
    col_type: &str,
) -> Result<(), rusqlite::Error> {
    if !column_exists(conn, table, column) {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {col_type}");
        conn.execute(&sql, [])?;
    }
    Ok(())
}
