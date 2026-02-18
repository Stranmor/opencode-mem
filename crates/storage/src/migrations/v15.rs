pub(super) const SQL: &str = "
DROP TABLE IF EXISTS observations_vec;
CREATE VIRTUAL TABLE IF NOT EXISTS observations_vec USING vec0(
    embedding float[1024] distance_metric=cosine
);
";
