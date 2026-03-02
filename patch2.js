const fs = require('fs');
let code = fs.readFileSync('crates/cli/src/commands/serve.rs', 'utf8');
code = code.replace(
    /axum::serve\(listener, router\.into_make_service_with_connect_info::<std::net::SocketAddr>\(\)\)\n\s*\.with_graceful_shutdown\(async move \{ let _ = shutdown_rx\.recv\(\)\.await; \}\)\n\s*\.await\?;/,
    `let mut is_restart = false;\n    axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>())\n        .with_graceful_shutdown(async move {\n            is_restart = shutdown_rx.recv().await.unwrap_or(false);\n        })\n        .await?;\n\n    if is_restart {\n        std::process::exit(1);\n    }`
);
fs.writeFileSync('crates/cli/src/commands/serve.rs', code);
