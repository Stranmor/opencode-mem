use crate::client::MAX_OUTPUT_LEN;

pub(crate) fn build_compression_prompt(
    tool: &str,
    title: &str,
    output: &str,
    candidates: &[opencode_mem_core::Observation],
) -> String {
    let mut types_prompt = String::new();
    for (i, variant) in opencode_mem_core::ObservationType::ALL_VARIANTS
        .iter()
        .enumerate()
    {
        types_prompt.push_str(&format!(
            "{}. {}: {}\n",
            i.saturating_add(1),
            variant.as_str().to_uppercase(),
            variant.description()
        ));
        for example in variant.examples() {
            types_prompt.push_str(&format!("   {}\n", example));
        }
        types_prompt.push('\n');
    }

    let existing_context = if candidates.is_empty() {
        "\n\nThere are no existing observations. You MUST use action: \"create\".".to_owned()
    } else {
        let mut entries = String::new();
        for (i, obs) in candidates.iter().enumerate() {
            let narrative_preview = obs
                .narrative
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();
            let facts_preview = if obs.facts.is_empty() {
                String::new()
            } else {
                format!(" facts=[{}]", obs.facts.join(", "))
            };
            entries.push_str(&format!(
                "[{}] id=\"{}\" type={} title=\"{}\" | {}{}\n",
                i.saturating_add(1),
                obs.id,
                obs.observation_type.as_str(),
                obs.title,
                narrative_preview,
                facts_preview,
            ));
        }
        format!(
            r#"

EXISTING OBSERVATIONS (potentially related):
{entries}
DECISION (MANDATORY — choose exactly one):
- If this is genuinely NEW knowledge not covered by any existing observation → action: "create"
- If this REFINES or ADDS TO an existing observation above → action: "update", target_id: "<id of the observation to update>"
- If this adds ZERO new information beyond what already exists → action: "skip""#
        )
    };

    let json_schema = if candidates.is_empty() {
        format!(
            r#"Return JSON:
- action: "create"
- noise_level: one of [{noise_levels}]
- noise_reason: why this is/isn't worth remembering (max 100 chars)
- type: one of [{obs_types}]
- type_reason: why this type and not another (max 80 chars). If type is "discovery", explain why it's not bugfix/change/decision/refactor.
- title: the lesson learned (max 80 chars, must be a complete statement of fact)
- subtitle: project/context this applies to
- narrative: the full lesson — what happened, why, and what to do differently
- facts: specific actionable facts (file paths, commands, error messages)
- concepts: from [{concepts}]
- files_read: file paths involved
- files_modified: file paths changed
- keywords: search terms"#,
            obs_types = opencode_mem_core::ObservationType::ALL_VARIANTS_STR,
            noise_levels = opencode_mem_core::NoiseLevel::ALL_VARIANTS_STR,
            concepts = opencode_mem_core::Concept::ALL_VARIANTS_STR
        )
    } else {
        format!(
            r#"Return JSON:
- action: one of "create", "update", "skip"
- target_id: id of existing observation to update (required if action is "update")
- skip_reason: why this should be skipped (required if action is "skip")
- noise_level: one of [{noise_levels}]
- noise_reason: why this is/isn't worth remembering (max 100 chars)
- type: one of [{obs_types}]
- type_reason: why this type and not another (max 80 chars). If type is "discovery", explain why it's not bugfix/change/decision/refactor.
- title: the lesson learned (max 80 chars, must be a complete statement of fact)
- subtitle: project/context this applies to
- narrative: the full lesson — what happened, why, and what to do differently
- facts: specific actionable facts (file paths, commands, error messages)
- concepts: from [{concepts}]
- files_read: file paths involved
- files_modified: file paths changed
- keywords: search terms"#,
            obs_types = opencode_mem_core::ObservationType::ALL_VARIANTS_STR,
            noise_levels = opencode_mem_core::NoiseLevel::ALL_VARIANTS_STR,
            concepts = opencode_mem_core::Concept::ALL_VARIANTS_STR
        )
    };

    format!(
        r#"You are a STRICT memory filter. Your job is to decide if this tool output contains a LESSON WORTH REMEMBERING across sessions.

Tool: {tool}
Output Title: {title}
Output Content: {output}

OBSERVATION TYPES — choose the MOST SPECIFIC type:

{types_prompt}
CLASSIFICATION DECISION TREE (follow in order):
1. Was a bug found AND fixed? → "bugfix" (must have root cause + fix)
2. Was an architectural or design choice made between alternatives? → "decision"
3. Was code restructured without changing behavior? → "refactor"
4. Was a new capability/endpoint/feature completed? → "feature"
5. Was configuration, deployment, or infrastructure changed? → "change"
6. Was something unexpected/surprising discovered about how code/API works? → "gotcha"
7. Did the user express a preference for how things should be done? → "preference"
8. Was genuinely new knowledge learned about how an existing system works? → "discovery"

"discovery" is the LAST resort, not the default. It means "I learned how something works" — not "something happened."

ANTI-DEFAULT RULE: If you choose type="discovery", you MUST prove it is not one of the 7 types above. Your type_reason field must explicitly state why bugfix/change/decision/refactor/feature/gotcha/preference do NOT apply. If you cannot prove this, pick the more specific type.

SKIP INSTRUCTION — return action: "skip" for ANY of these:
- Status updates: "published X", "deployed Y", "completed task Z", "merged PR"
- Progress reports: "finished step 3 of 5", "working on feature X"
- Transactional confirmations: "file saved", "test passed", "build succeeded"
- Routine file reads/writes with no surprising content
- Metadata: "database has N records", "N files changed"
- Generic knowledge available in any documentation (e.g. "Rust uses ownership")
These have ZERO information value for future sessions. Do not create observations for them.
{existing_context}
NOISE LEVEL GUIDE (5 levels — be precise, "medium" is NOT a safe default):
- "critical": Production outage, data loss, security vulnerability, core architectural decision that affects the entire system. Would cause system failure if ignored.
- "high": Bugfix with root cause analysis that saves hours of debugging, significant gotcha/pitfall, architectural decision with clear tradeoffs, user preference that changes workflow.
- "medium": Non-obvious implementation detail, minor gotcha worth noting, configuration that has side effects. ONLY if it would concretely help a future agent — not "might be useful someday."
- "low": Environment-specific workaround, minor configuration tweak, observation that is correct but unlikely to be needed again.
- "negligible": Routine work, generic knowledge, status updates, file edits, build output, duplicates. DISCARD.

ANTI-DEFAULT RULE: If you choose noise_level="medium", you MUST explain in noise_reason what SPECIFIC future scenario this observation would help with. "Might be useful" or "good to know" are NOT valid reasons. If you cannot name a concrete scenario, use "low" or "negligible."

{json_schema}"#,
        tool = tool,
        title = title,
        output = opencode_mem_core::truncate(output, MAX_OUTPUT_LEN),
        types_prompt = types_prompt,
        existing_context = existing_context,
        json_schema = json_schema,
    )
}
