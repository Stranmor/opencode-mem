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
    MemoryDelete,
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
    pub fn all_tool_names() -> &'static [&'static str] {
        &[
            "__IMPORTANT",
            "search",
            "timeline",
            "get_observations",
            "memory_get",
            "memory_recent",
            "memory_hybrid_search",
            "memory_semantic_search",
            "save_memory",
            "memory_delete",
            "knowledge_search",
            "knowledge_save",
            "knowledge_get",
            "knowledge_list",
            "knowledge_delete",
            "infinite_expand",
            "infinite_time_range",
            "infinite_drill_hour",
            "infinite_drill_minute",
        ]
    }

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
            "memory_delete" => Some(Self::MemoryDelete),
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
1. search(query) → Get index with IDs (~50-100 tokens/result)
2. timeline(from/to) → Get context around interesting results  
3. get_observations([IDs]) → Fetch full details ONLY for filtered IDs
NEVER fetch full details without filtering first. 10x token savings.";
