//! Migration v8: Global Knowledge Layer

pub const SQL: &str = r#"
CREATE TABLE IF NOT EXISTS global_knowledge (
    id TEXT PRIMARY KEY,
    knowledge_type TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    instructions TEXT,
    triggers TEXT NOT NULL DEFAULT '[]',
    source_projects TEXT NOT NULL DEFAULT '[]',
    source_observations TEXT NOT NULL DEFAULT '[]',
    confidence REAL NOT NULL DEFAULT 0.5,
    usage_count INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_type ON global_knowledge(knowledge_type);
CREATE INDEX IF NOT EXISTS idx_knowledge_confidence ON global_knowledge(confidence);

CREATE VIRTUAL TABLE IF NOT EXISTS global_knowledge_fts USING fts5(
    title,
    description,
    instructions,
    triggers,
    content='global_knowledge',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS global_knowledge_ai AFTER INSERT ON global_knowledge BEGIN
    INSERT INTO global_knowledge_fts(rowid, title, description, instructions, triggers)
    VALUES (NEW.rowid, NEW.title, NEW.description, NEW.instructions, NEW.triggers);
END;

CREATE TRIGGER IF NOT EXISTS global_knowledge_ad AFTER DELETE ON global_knowledge BEGIN
    INSERT INTO global_knowledge_fts(global_knowledge_fts, rowid, title, description, instructions, triggers)
    VALUES ('delete', OLD.rowid, OLD.title, OLD.description, OLD.instructions, OLD.triggers);
END;

CREATE TRIGGER IF NOT EXISTS global_knowledge_au AFTER UPDATE ON global_knowledge BEGIN
    INSERT INTO global_knowledge_fts(global_knowledge_fts, rowid, title, description, instructions, triggers)
    VALUES ('delete', OLD.rowid, OLD.title, OLD.description, OLD.instructions, OLD.triggers);
    INSERT INTO global_knowledge_fts(rowid, title, description, instructions, triggers)
    VALUES (NEW.rowid, NEW.title, NEW.description, NEW.instructions, NEW.triggers);
END;
"#;
