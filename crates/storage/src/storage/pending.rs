use anyhow::Result;
use chrono::Utc;
use rusqlite::params;

use super::{get_conn, log_row_error, Storage};
use crate::pending_queue::{max_retry_count, PendingMessage, PendingMessageStatus, QueueStats};

fn row_to_pending_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<PendingMessage> {
    let status_str: String = row.get(2)?;
    let status =
        status_str.parse::<PendingMessageStatus>().unwrap_or(PendingMessageStatus::Pending);
    Ok(PendingMessage {
        id: row.get(0)?,
        session_id: row.get(1)?,
        status,
        tool_name: row.get(3)?,
        tool_input: row.get(4)?,
        tool_response: row.get(5)?,
        retry_count: row.get(6)?,
        created_at_epoch: row.get(7)?,
        claimed_at_epoch: row.get(8)?,
        completed_at_epoch: row.get(9)?,
        project: row.get(10)?,
    })
}

impl Storage {
    /// Queue a message for processing.
    ///
    /// # Errors
    /// Returns error if database insert fails.
    pub fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64> {
        let conn = get_conn(&self.pool)?;
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO pending_messages 
               (session_id, status, tool_name, tool_input, tool_response, retry_count, created_at_epoch, project)
               VALUES (?1, 'pending', ?2, ?3, ?4, 0, ?5, ?6)",
            params![session_id, tool_name, tool_input, tool_response, now, project],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Claim pending messages for processing.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    pub fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>> {
        let conn = get_conn(&self.pool)?;
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;

        let mut stmt = conn.prepare(
            "UPDATE pending_messages
               SET status = 'processing', claimed_at_epoch = ?1
               WHERE id IN (
                   SELECT id FROM pending_messages
                   WHERE status = 'pending'
                      OR (status = 'processing' AND claimed_at_epoch < ?2)
                   ORDER BY created_at_epoch ASC
                   LIMIT ?3
               )
               RETURNING id, session_id, status, tool_name, tool_input, tool_response,
                          retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project",
        )?;

        let messages: Vec<PendingMessage> = stmt
            .query_map(params![now, stale_threshold, limit], row_to_pending_message)?
            .filter_map(log_row_error)
            .collect();

        Ok(messages)
    }

    /// Delete message after successful processing.
    /// Raw tool outputs may contain secrets - we don't keep them after processing.
    pub fn complete_message(&self, id: i64) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute("DELETE FROM pending_messages WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Mark message as failed.
    ///
    /// # Errors
    /// Returns error if database update fails.
    pub fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        if increment_retry {
            conn.execute(
                "UPDATE pending_messages 
                   SET retry_count = retry_count + 1,
                       status = CASE 
                           WHEN retry_count + 1 >= ?1 THEN 'failed'
                           ELSE 'pending'
                       END,
                       claimed_at_epoch = NULL
                   WHERE id = ?2",
                params![max_retry_count(), id],
            )?;
        } else {
            conn.execute(
                "UPDATE pending_messages SET status = 'failed' WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }

    /// Get count of pending messages.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_pending_count(&self) -> Result<usize> {
        let conn = get_conn(&self.pool)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pending_messages WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Release stale processing messages.
    ///
    /// # Errors
    /// Returns error if database update fails.
    pub fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize> {
        let conn = get_conn(&self.pool)?;
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;
        let affected = conn.execute(
            "UPDATE pending_messages 
               SET status = 'pending', claimed_at_epoch = NULL
               WHERE status = 'processing' AND claimed_at_epoch <= ?1",
            params![stale_threshold],
        )?;
        Ok(affected)
    }

    /// Get failed messages.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, status, tool_name, tool_input, tool_response,
                      retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project
               FROM pending_messages
               WHERE status = 'failed'
               ORDER BY created_at_epoch DESC
               LIMIT ?1",
        )?;
        let results = stmt
            .query_map(params![limit], row_to_pending_message)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Get all pending messages.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, status, tool_name, tool_input, tool_response,
                      retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project
               FROM pending_messages
               ORDER BY created_at_epoch DESC
               LIMIT ?1",
        )?;
        let results = stmt
            .query_map(params![limit], row_to_pending_message)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Get queue statistics.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_queue_stats(&self) -> Result<QueueStats> {
        let conn = get_conn(&self.pool)?;
        let (pending, processing, failed, processed): (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
        ) = conn.query_row(
            "SELECT 
                SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'processed' THEN 1 ELSE 0 END)
            FROM pending_messages",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        Ok(QueueStats {
            pending: pending.unwrap_or(0) as u64,
            processing: processing.unwrap_or(0) as u64,
            failed: failed.unwrap_or(0) as u64,
            processed: processed.unwrap_or(0) as u64,
        })
    }

    /// Clear all failed messages.
    ///
    /// # Errors
    /// Returns error if database delete fails.
    pub fn clear_failed_messages(&self) -> Result<usize> {
        let conn = get_conn(&self.pool)?;
        let affected = conn.execute("DELETE FROM pending_messages WHERE status = 'failed'", [])?;
        Ok(affected)
    }

    /// Clear all pending messages.
    ///
    /// # Errors
    /// Returns error if database delete fails.
    pub fn clear_all_pending_messages(&self) -> Result<usize> {
        let conn = get_conn(&self.pool)?;
        let affected = conn.execute("DELETE FROM pending_messages", [])?;
        Ok(affected)
    }
}
