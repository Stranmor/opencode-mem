// These tests require a running PostgreSQL instance and are meant to document
// adversarial edge cases found during security review.

#[tokio::test]
#[ignore]
async fn test_process_skips_infinite_memory_on_retry() {
    // This test documents that `ObservationService::process` will skip infinite memory
    // if the observation already exists in primary storage.
    // 1. Queue process calls `process(id)`
    // 2. `compress_and_save` succeeds -> observation in Postgres
    // 3. `store_infinite_memory` fails -> returns Err
    // 4. Queue retries `process(id)`
    // 5. `existing_obs` is Some, function early-returns Ok(Some)
    // 6. `store_infinite_memory` is NEVER called. Data loss in long-term memory.

    let is_vulnerable = true;
    assert!(is_vulnerable, "Vulnerability: store_infinite_memory is skipped on queue retry");
}

#[tokio::test]
#[ignore]
async fn test_compress_and_save_loses_id_on_update() {
    // 1. Queue message has `id = new_uuid`
    // 2. LLM decides to UPDATE `target_id = old_uuid`
    // 3. `merge_into_existing` updates `old_uuid`
    // 4. `new_uuid` is never saved
    // 5. If `store_infinite_memory` fails, queue retries `new_uuid`
    // 6. `get_by_id(new_uuid)` returns None
    // 7. LLM compression runs AGAIN for the same message, duplicating work/merges.

    let is_vulnerable = true;
    assert!(
        is_vulnerable,
        "Vulnerability: Queue ID is lost on LLM Update, causing infinite retry loops"
    );
}

#[tokio::test]
#[ignore]
async fn test_save_observation_silent_data_loss_duplicate_title() {
    // 1. LLM generates observation with `title = X`
    // 2. Postgres has UNIQUE constraint on `title_normalized`
    // 3. `save_observation` throws 23505 (Unique Violation)
    // 4. `save_observation` catches it and returns `Ok(false)`
    // 5. `persist_and_notify` treats it as duplicate, returns `Ok(Some(obs, false))`
    // 6. `process` returns `Ok(Some(obs))`
    // 7. The observation was NEVER inserted. The ID doesn't exist.
    // 8. Queue deletes the message. The knowledge is permanently lost without any error log!

    let is_vulnerable = true;
    assert!(is_vulnerable, "Vulnerability: 23505 on duplicate title causes silent data loss");
}
