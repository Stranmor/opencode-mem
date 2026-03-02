const fs = require('fs');
let code = fs.readFileSync('crates/http/src/handlers/queue.rs', 'utf8');
code = code.replace(
    /use futures::FutureExt;/,
    `use futures_util::FutureExt;`
);
fs.writeFileSync('crates/http/src/handlers/queue.rs', code);
