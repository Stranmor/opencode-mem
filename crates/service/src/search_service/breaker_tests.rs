
#[tokio::test]
#[ignore = "Demonstrates vulnerability #128: try_embed(?)-propagation bypasses fallback"]
async fn test_hybrid_search_fails_on_embedding_error_instead_of_fallback() {
    // This test documents that `SearchService::hybrid_search` and `search_with_filters`
    // do NOT fall back to text-only search if embedding generation fails, causing a DoS.
    // 1. `try_embed` returns `Err` on embedding generation failure (e.g. OOM, bad model, string too long).
    // 2. `run_hybrid_search` uses `self.try_embed(query).await?`.
    // 3. The `?` propagates the `Err` immediately instead of matching on it.
    // 4. Text-only `hybrid_search` fallback is never reached.
    // 5. User receives a 500 error instead of degraded search results.
    let is_vulnerable = true;
    assert!(
        is_vulnerable,
        "Vulnerability: Embedding failure crashes hybrid_search instead of falling back to text search"
    );
}
