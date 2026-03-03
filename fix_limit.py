with open('crates/mcp/src/handlers/mod.rs', 'r') as f:
    text = f.read()

old_parse = """fn parse_limit(args: &serde_json::Value) -> usize {
    args.get("limit")
        .and_then(serde_json::Value::as_u64)
        .map(|l| l as usize)
        .unwrap_or(opencode_mem_core::DEFAULT_QUERY_LIMIT)
}"""

new_parse = """fn parse_limit(args: &serde_json::Value, default_val: usize) -> usize {
    args.get("limit")
        .and_then(serde_json::Value::as_u64)
        .map(|l| l as usize)
        .unwrap_or(default_val)
}"""

text = text.replace(old_parse, new_parse)

text = text.replace('parse_limit(args)', 'parse_limit(args, opencode_mem_core::DEFAULT_QUERY_LIMIT)')

text = text.replace('memory::handle_search(search_service, &args).await', 'memory::handle_search(search_service, &args, parse_limit(&args, 50)).await')
text = text.replace('memory::handle_timeline(search_service, &args).await', 'memory::handle_timeline(search_service, &args, parse_limit(&args, 50)).await')
text = text.replace('memory::handle_memory_recent(search_service, &args).await', 'memory::handle_memory_recent(search_service, &args, parse_limit(&args, 10)).await')
text = text.replace('memory::handle_hybrid_search(search_service, &args).await', 'memory::handle_hybrid_search(search_service, &args, parse_limit(&args, 50)).await')
text = text.replace('memory::handle_semantic_search(search_service, &args).await', 'memory::handle_semantic_search(search_service, &args, parse_limit(&args, 50)).await')
text = text.replace('knowledge::handle_knowledge_search(knowledge_service, &args).await', 'knowledge::handle_knowledge_search(knowledge_service, &args, parse_limit(&args, 10)).await')
text = text.replace('knowledge::handle_knowledge_list(knowledge_service, &args).await', 'knowledge::handle_knowledge_list(knowledge_service, &args, parse_limit(&args, 10)).await')

with open('crates/mcp/src/handlers/mod.rs', 'w') as f:
    f.write(text)
