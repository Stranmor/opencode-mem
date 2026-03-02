const fs = require('fs');
let code = fs.readFileSync('crates/http/src/handlers/queue.rs', 'utf8');
code = code.replace(
    /let result = process_pending_message\(&state_clone, &msg\)\.await;/,
    `use futures::FutureExt;\n            let result = std::panic::AssertUnwindSafe(process_pending_message(&state_clone, &msg))\n                .catch_unwind()\n                .await\n                .unwrap_or_else(|_| Err(anyhow::anyhow!("Panic during message processing")));`
);
fs.writeFileSync('crates/http/src/handlers/queue.rs', code);
