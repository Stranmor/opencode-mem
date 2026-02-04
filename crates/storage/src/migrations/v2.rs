//! Migration v2: FTS5 for observations

pub(super) const SQL: &str = "
DROP TABLE IF EXISTS observations_fts;

CREATE VIRTUAL TABLE observations_fts USING fts5(
    title, subtitle, narrative, facts, keywords,
    content='observations',
    content_rowid='rowid'
);

DROP TRIGGER IF EXISTS observations_ai;
CREATE TRIGGER observations_ai AFTER INSERT ON observations BEGIN
    INSERT INTO observations_fts(rowid, title, subtitle, narrative, facts, keywords)
    VALUES (new.rowid, new.title, new.subtitle, new.narrative, new.facts, new.keywords);
END;

INSERT INTO observations_fts(rowid, title, subtitle, narrative, facts, keywords)
SELECT rowid, title, subtitle, narrative, facts, keywords FROM observations;
";
