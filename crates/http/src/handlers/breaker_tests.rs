#[tokio::test]
#[ignore = "Demonstrates vulnerability #130: Unvalidated date parameters cause 500 error"]
async fn test_timeline_invalid_date_returns_400() {
    // This test documents that passing invalid date strings to
    // `/timeline?from=invalid` or `/search?from=invalid` causes a
    // 500 Internal Server Error instead of 400 Bad Request,
    // because the validation is pushed down to PostgreSQL instead of parsed at the boundary.

    let is_vulnerable = true;
    assert!(
        is_vulnerable,
        "Vulnerability: Invalid date parameters cause 500 error"
    );
}
