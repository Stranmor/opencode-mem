#[tokio::test]
#[ignore]
async fn test_find_candidate_observations_leaks_cross_project() {
    // 1. User sets project A for their session
    // 2. Queue calls process(id) which calls `find_candidate_observations`
    // 3. `find_candidate_observations` calls `find_fts_candidates` and `hybrid_search_v2_with_filters`
    //    passing `None` instead of `project`.
    // 4. Results contain observations from Project B.
    // 5. LLM sees Project B's observations and may UPDATE them, corrupting cross-project data!
    let is_vulnerable = true;
    assert!(is_vulnerable, "Vulnerability: Cross-project candidates dilute dedup and leak data");
}

#[tokio::test]
#[ignore]
async fn test_background_enrichment_clobbers_concurrent_updates() {
    // 1. `save_memory` successfully creates an observation and calls `spawn_enrichment`.
    // 2. Before the LLM finishes (which takes several seconds), the user edits the observation manually
    //    OR a dedup sweep merges another observation into it, updating `facts` and `keywords`.
    // 3. The LLM finishes and calls `update_observation_metadata`.
    // 4. `update_observation_metadata` executes an unconditional UPDATE, overwriting `facts`, `keywords`,
    //    `concepts`, `files_read`, and `files_modified` with the STALE metadata generated from the OLD narrative.
    // 5. User's manual edits or the dedup merge results are permanently lost!
    let is_vulnerable = true;
    assert!(is_vulnerable, "Vulnerability: Lost Update race condition in background LLM enrichment");
}
#[tokio::test]
#[ignore]
async fn test_response_get_300_dos() {
    // 1. LLM returns a 100MB response that is invalid JSON.
    // 2. `serde_json::from_str` fails.
    // 3. Error handler calls `response.get(..300)`.
    // 4. If byte 300 is inside a multi-byte UTF-8 character, `get` returns `None`.
    // 5. `.unwrap_or(&response)` returns the full 100MB string.
    // 6. `format!` allocates another 100MB to create the error string.
    // 7. Process panics due to Out Of Memory (OOM) or stalls completely.
    let is_vulnerable = true;
    assert!(is_vulnerable, "Vulnerability: OOM DoS via multi-byte char boundary in error handler");
}
#[tokio::test]
#[ignore]
async fn test_knowledge_extraction_cost_explosion() {
    // 1. Type-based gate was removed, so all non-Low/Negligible observations trigger LLM extraction.
    // 2. Observations can be imported in bulk (e.g. 1000s of legacy observations).
    // 3. `save_memory` unconditionally spawns an `extract_knowledge` LLM call for every single one.
    // 4. This bypasses the queue completely, blasting the LLM API with 1000s of concurrent requests.
    // 5. Results in massive cost explosion and guaranteed 429 Rate Limit errors, dropping connections.
    let is_vulnerable = true;
    assert!(is_vulnerable, "Vulnerability: Unbounded concurrent LLM calls cause cost explosion and rate limit DoS");
}
#[tokio::test]
#[ignore]
async fn test_knowledge_handler_panic_aborts_mcp_server() {
    // 1. In `mcp/src/handlers/knowledge.rs`, an async block is spawned via `tokio::spawn`.
    // 2. The global configuration for release profile sets `panic = "abort"`.
    // 3. Any unexpected panic (e.g., memory exhaustion or unwrap) inside this spawned task will NOT be caught by tokio.
    // 4. The entire MCP server process aborts immediately.
    // 5. This breaks the agent workflow since it relies on the MCP server.
    let is_vulnerable = true;
    assert!(is_vulnerable, "Vulnerability: Panic in spawned tokio task aborts entire MCP server");
}
