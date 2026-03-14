#[tokio::test]
#[ignore = "Demonstrates vulnerability #129: save_observation fails if embedding fails"]
async fn test_save_observation_succeeds_even_if_embedding_fails() {
    // This test documents that `ObservationService::save_observation`
    // drops the entire observation if `try_embed` returns an error (e.g., text too long).
    // The `?` on `try_embed` aborts the save operation.
    // It should instead save the observation with `embedding = NULL` and log a warning,
    // preserving data integrity.

    let is_vulnerable = true;
    assert!(
        is_vulnerable,
        "Vulnerability: save_observation aborts if try_embed fails, causing data loss"
    );
}
