//! Static API documentation data.

use crate::api_types::{EndpointDoc, ParamDoc, SearchHelpResponse};

/// Returns API documentation for search-related endpoints.
pub fn get_search_help() -> SearchHelpResponse {
    SearchHelpResponse {
        endpoints: vec![
            EndpointDoc {
                path: "/api/unified-search",
                method: "GET",
                description: "Unified search across observations, sessions, and prompts",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: true,
                        description: "Search query",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results (default 20)",
                    },
                    ParamDoc {
                        name: "project",
                        required: false,
                        description: "Filter by project",
                    },
                    ParamDoc {
                        name: "type",
                        required: false,
                        description: "Filter by observation type",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/unified-timeline",
                method: "GET",
                description: "Get timeline centered around an anchor observation",
                params: vec![
                    ParamDoc {
                        name: "anchor",
                        required: false,
                        description: "Observation ID to center on",
                    },
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Search query to find anchor",
                    },
                    ParamDoc {
                        name: "before",
                        required: false,
                        description: "Count before anchor (default 5)",
                    },
                    ParamDoc {
                        name: "after",
                        required: false,
                        description: "Count after anchor (default 5)",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/decisions",
                method: "GET",
                description: "Get observations of type 'decision'",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Optional search filter",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/changes",
                method: "GET",
                description: "Get observations of type 'change'",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Optional search filter",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/how-it-works",
                method: "GET",
                description: "Search for 'how-it-works' concept observations",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Additional search terms",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/context/preview",
                method: "GET",
                description: "Generate context preview for a project",
                params: vec![
                    ParamDoc {
                        name: "project",
                        required: true,
                        description: "Project path",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max observations",
                    },
                    ParamDoc {
                        name: "format",
                        required: false,
                        description: "'compact' or 'full'",
                    },
                ],
            },
        ],
    }
}
