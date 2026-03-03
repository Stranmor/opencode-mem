with open('crates/infinite-memory/src/event_types.rs', 'r') as f:
    text = f.read()

new_struct = """#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct SummaryEntities {
    pub files: Vec<String>,
    pub functions: Vec<String>,
    pub libraries: Vec<String>,
    pub errors: Vec<String>,
    pub decisions: Vec<String>,
}

impl SummaryEntities {
    pub const ALL_KEYS: &'static [&'static str] = &["files", "functions", "libraries", "errors", "decisions"];
    
    pub fn prompt_schema() -> String {
        format!(
            r#"{{
  "summary": "Краткое описание на русском (2-3 предложения)",
  "entities": {{
    "{}": ["список изменённых файлов"],
    "{}": ["упомянутые функции"],
    "{}": ["внешние библиотеки"],
    "{}": ["типы ошибок"],
    "{}": ["ключевые решения"]
  }}
}}"#,
            Self::ALL_KEYS[0], Self::ALL_KEYS[1], Self::ALL_KEYS[2], Self::ALL_KEYS[3], Self::ALL_KEYS[4]
        )
    }"""

old_struct = """#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct SummaryEntities {
    pub files: Vec<String>,
    pub functions: Vec<String>,
    pub libraries: Vec<String>,
    pub errors: Vec<String>,
    pub decisions: Vec<String>,
}

impl SummaryEntities {"""

text = text.replace(old_struct, new_struct)
with open('crates/infinite-memory/src/event_types.rs', 'w') as f:
    f.write(text)

with open('crates/infinite-memory/src/compression.rs', 'r') as f:
    text = f.read()

old_prompt = """    let prompt = format!(
        r#"Проанализируй эти {} событий и верни JSON:
{{
  "summary": "Краткое описание на русском (2-3 предложения)",
  "entities": {{
    "files": ["список изменённых файлов"],
    "functions": ["упомянутые функции"],
    "libraries": ["внешние библиотеки"],
    "errors": ["типы ошибок"],
    "decisions": ["ключевые решения"]
  }}
}}

События:
{}"#,
        events.len(),
        events_text.join("\n")
    );"""

new_prompt = """    let prompt = format!(
        "Проанализируй эти {} событий и верни JSON:\n{}\n\nСобытия:\n{}",
        events.len(),
        SummaryEntities::prompt_schema(),
        events_text.join("\n")
    );"""

text = text.replace(old_prompt, new_prompt)
with open('crates/infinite-memory/src/compression.rs', 'w') as f:
    f.write(text)

with open('crates/infinite-memory/src/summary_queries.rs', 'r') as f:
    text = f.read()

text = text.replace(
    'const ALLOWED_TYPES: &[&str] = &["files", "functions", "libraries", "errors", "decisions"];\n    if !ALLOWED_TYPES.contains(&entity_type) {',
    'if !crate::event_types::SummaryEntities::ALL_KEYS.contains(&entity_type) {'
)
text = text.replace(
    'anyhow::bail!("Invalid entity_type \'{}\'. Allowed: {:?}", entity_type, ALLOWED_TYPES);',
    'anyhow::bail!("Invalid entity_type \'{}\'. Allowed: {:?}", entity_type, crate::event_types::SummaryEntities::ALL_KEYS);'
)

with open('crates/infinite-memory/src/summary_queries.rs', 'w') as f:
    f.write(text)

