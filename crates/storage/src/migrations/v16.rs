pub(super) const SQL: &str = "
CREATE TABLE IF NOT EXISTS injected_observations (
    session_id TEXT NOT NULL,
    observation_id TEXT NOT NULL,
    injected_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (session_id, observation_id)
);
CREATE INDEX IF NOT EXISTS idx_injected_obs_session ON injected_observations(session_id);
";
