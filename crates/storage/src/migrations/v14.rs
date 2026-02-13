pub(super) const SQL: &str = "
DROP TRIGGER IF EXISTS observations_au;
CREATE TRIGGER observations_au AFTER UPDATE ON observations BEGIN
    INSERT INTO observations_fts(observations_fts, rowid, title, subtitle, narrative, facts, keywords)
    VALUES ('delete', old.rowid, old.title, old.subtitle, old.narrative, old.facts, old.keywords);
    INSERT INTO observations_fts(rowid, title, subtitle, narrative, facts, keywords)
    VALUES (new.rowid, new.title, new.subtitle, new.narrative, new.facts, new.keywords);
END;

DROP TRIGGER IF EXISTS observations_ad;
CREATE TRIGGER observations_ad AFTER DELETE ON observations BEGIN
    INSERT INTO observations_fts(observations_fts, rowid, title, subtitle, narrative, facts, keywords)
    VALUES ('delete', old.rowid, old.title, old.subtitle, old.narrative, old.facts, old.keywords);
END;
";
