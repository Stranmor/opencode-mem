const fs = require('fs');
const path = '/home/stranmor/.config/opencode/plugins/memory-bridge.ts';
let content = fs.readFileSync(path, 'utf8');

content = content.replace(
  /const results: SearchResult\[\] = await response\.json\(\)/g,
  `const text = await response.text()
        if (!text) return

        let results: SearchResult[]
        try {
          results = JSON.parse(text)
        } catch (parseErr) {
          console.warn("[memory-bridge] failed to parse JSON, raw text:", text)
          return
        }`
);

fs.writeFileSync(path, content);
