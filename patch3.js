const fs = require('fs');
let code = fs.readFileSync('crates/cli/src/commands/serve.rs', 'utf8');
code = code.replace(
    /let mut is_restart = false;\n    axum::serve\(listener, router\.into_make_service_with_connect_info::<std::net::SocketAddr>\(\)\)\n        \.with_graceful_shutdown\(async move \{\n            is_restart = shutdown_rx\.recv\(\)\.await\.unwrap_or\(false\);\n        \}\)\n        \.await\?;\n\n    if is_restart \{/,
    `let is_restart = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));\n    let is_restart_clone = is_restart.clone();\n    axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>())\n        .with_graceful_shutdown(async move {\n            is_restart_clone.store(shutdown_rx.recv().await.unwrap_or(false), std::sync::atomic::Ordering::Relaxed);\n        })\n        .await?;\n\n    if is_restart.load(std::sync::atomic::Ordering::Relaxed) {`
);
fs.writeFileSync('crates/cli/src/commands/serve.rs', code);
