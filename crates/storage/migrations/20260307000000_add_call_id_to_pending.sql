-- Add call_id to pending_messages for idempotent tool execution tracking
ALTER TABLE pending_messages ADD COLUMN call_id TEXT;
