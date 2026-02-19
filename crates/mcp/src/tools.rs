use serde_json::json;

/// All MCP tools exposed by this server.
/// Using an enum ensures compile-time safety for tool names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTool {
    Important,
    Search,
    Timeline,
    GetObservations,
    MemoryGet,
    MemoryRecent,
    MemoryHybridSearch,
    MemorySemanticSearch,
    SaveMemory,
    KnowledgeSearch,
    KnowledgeSave,
    KnowledgeGet,
    KnowledgeList,
    KnowledgeDelete,
    InfiniteExpand,
    InfiniteTimeRange,
    InfiniteDrillHour,
    InfiniteDrillMinute,
}

impl McpTool {
    /// Parse tool name from JSON-RPC request.
    /// Returns None for unknown tools (caller must handle error).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "__IMPORTANT" => Some(Self::Important),
            "search" => Some(Self::Search),
            "timeline" => Some(Self::Timeline),
            "get_observations" => Some(Self::GetObservations),
            "memory_get" => Some(Self::MemoryGet),
            "memory_recent" => Some(Self::MemoryRecent),
            "memory_hybrid_search" => Some(Self::MemoryHybridSearch),
            "memory_semantic_search" => Some(Self::MemorySemanticSearch),
            "save_memory" => Some(Self::SaveMemory),
            "knowledge_search" => Some(Self::KnowledgeSearch),
            "knowledge_save" => Some(Self::KnowledgeSave),
            "knowledge_get" => Some(Self::KnowledgeGet),
            "knowledge_list" => Some(Self::KnowledgeList),
            "knowledge_delete" => Some(Self::KnowledgeDelete),
            "infinite_expand" => Some(Self::InfiniteExpand),
            "infinite_time_range" => Some(Self::InfiniteTimeRange),
            "infinite_drill_hour" => Some(Self::InfiniteDrillHour),
            "infinite_drill_minute" => Some(Self::InfiniteDrillMinute),
            _ => None,
        }
    }
}

pub const WORKFLOW_DOCS: &str = r"3-LAYER WORKFLOW (ALWAYS FOLLOW):
1. search(query) \u{2192} Get index with IDs (~50-100 tokens/result)
2. timeline(from/to) \u{2192} Get context around interesting results  
3. get_observations([IDs]) \u{2192} Fetch full details ONLY for filtered IDs
NEVER fetch full details without filtering first. 10x token savings.";

/// Returns the JSON schema for all MCP tools.
pub fn get_tools_json() -> serde_json::Value {
    json!({
        "tools": [
            {
                "name": "__IMPORTANT",
                "description": WORKFLOW_DOCS,
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "search",
                "description": "Step 1: Search memory. Returns index with IDs. Uses semantic search when available, falls back to text search. Params: query (required), limit, project, type, from, to",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "limit": { "type": "integer", "default": 50 },
                        "project": { "type": "string", "description": "Filter by project" },
                        "type": { "type": "string", "description": "Filter by observation type (bugfix, feature, refactor, discovery, decision, change)" },
                        "from": { "type": "string", "description": "Start date (ISO 8601)" },
                        "to": { "type": "string", "description": "End date (ISO 8601)" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "timeline",
                "description": "Step 2: Get chronological context. Params: from, to, limit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Start date (ISO 8601)" },
                        "to": { "type": "string", "description": "End date (ISO 8601)" },
                        "limit": { "type": "integer", "default": 50 }
                    }
                }
            },
            {
                "name": "get_observations",
                "description": "Step 3: Fetch full details for filtered IDs. Always batch multiple IDs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Array of observation IDs to fetch (required)"
                        }
                    },
                    "required": ["ids"]
                }
            },
            {
                "name": "memory_get",
                "description": "Get full observation details by ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Observation ID" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "memory_recent",
                "description": "Get recent observations",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "default": 10 }
                    }
                }
            },
            {
                "name": "memory_hybrid_search",
                "description": "Hybrid search combining FTS and keyword matching",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query (supports multiple words)" },
                        "limit": { "type": "integer", "default": 20 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "memory_semantic_search",
                "description": "Smart search with semantic understanding when embeddings available, falls back to hybrid FTS+keyword search otherwise. Best for finding conceptually related content.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "limit": { "type": "integer", "default": 20 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "save_memory",
                "description": "Save memory directly without LLM compression",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Memory text to save" },
                        "title": { "type": "string", "description": "Optional title (defaults to first 50 chars of text)" },
                        "project": { "type": "string", "description": "Optional project to associate with this memory" }
                    },
                    "required": ["text"]
                }
            },
            {
                "name": "knowledge_search",
                "description": "Search global knowledge base for skills, patterns, gotchas",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "limit": { "type": "integer", "default": 10 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "knowledge_save",
                "description": "Save new knowledge entry (skill, pattern, gotcha, architecture, tool_usage)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "knowledge_type": { "type": "string", "enum": ["skill", "pattern", "gotcha", "architecture", "tool_usage"] },
                        "title": { "type": "string" },
                        "description": { "type": "string" },
                        "instructions": { "type": "string", "description": "Step-by-step instructions (for skills)" },
                        "triggers": { "type": "array", "items": { "type": "string" }, "description": "Keywords/contexts when to use" },
                        "source_project": { "type": "string" },
                        "source_observation": { "type": "string" }
                    },
                    "required": ["knowledge_type", "title", "description"]
                }
            },
            {
                "name": "knowledge_get",
                "description": "Get knowledge entry by ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "knowledge_list",
                "description": "List knowledge entries, optionally filtered by type",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "knowledge_type": { "type": "string", "enum": ["skill", "pattern", "gotcha", "architecture", "tool_usage"] },
                        "limit": { "type": "integer", "default": 20 }
                    }
                }
            },
            {
                "name": "knowledge_delete",
                "description": "Delete knowledge entry by ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "infinite_expand",
                "description": "Expand a summary to see its child events. Drills down from any summary level to raw events.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer", "description": "Summary ID to expand" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "infinite_time_range",
                "description": "Get events within a time range. Returns raw events or summaries depending on granularity.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "start": { "type": "string", "description": "Start time (ISO 8601)" },
                        "end": { "type": "string", "description": "End time (ISO 8601)" },
                        "session_id": { "type": "string", "description": "Optional session filter" }
                    },
                    "required": ["start", "end"]
                }
            },
            {
                "name": "infinite_drill_hour",
                "description": "Drill down from a day summary to its hour summaries.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer", "description": "Day summary ID" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "infinite_drill_minute",
                "description": "Drill down from an hour summary to its 5-minute summaries.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer", "description": "Hour summary ID" }
                    },
                    "required": ["id"]
                }
            }
        ]
    })
}
