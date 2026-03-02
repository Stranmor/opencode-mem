const fs = require('fs');
let code = fs.readFileSync('crates/cli/src/commands/serve.rs', 'utf8');
code = code.replace(
    /axum::serve\(listener, router\.into_make_service_with_connect_info::<std::net::SocketAddr>\(\)\)\n\s*\.await\?;\n\n\s*Ok\(\(\)\)/,
    `axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>())\n        .with_graceful_shutdown(async move { let _ = shutdown_rx.recv().await; })\n        .await?;\n\n    Ok(())`
);
fs.writeFileSync('crates/cli/src/commands/serve.rs', code);
