use super::is_localhost;
use crate::api_error::ApiError;
use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::task::spawn_blocking;

use crate::AppState;
use crate::api_types::{
    AdminResponse, InstructionsQuery, InstructionsResponse, McpStatusResponse, SettingsResponse,
    ToggleMcpRequest, UpdateSettingsRequest,
};

pub async fn get_settings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SettingsResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let mut settings = state.settings.read().await.clone();

    redact_sensitive_env(&mut settings.env);

    Ok(Json(SettingsResponse { settings }))
}

fn redact_sensitive_env(env: &mut std::collections::HashMap<String, String>) {
    for (key, value) in env.iter_mut() {
        let k = key.to_uppercase();
        if k.contains("KEY")
            || k.contains("SECRET")
            || k.contains("PASSWORD")
            || k.contains("TOKEN")
            || k.contains("AUTH")
            || k.contains("CREDENTIAL")
            || k.contains("PRIVATE")
            || k.contains("PASS")
            || k.contains("JWT")
            || (k.ends_with("_URL") && value.contains('@'))
        {
            *value = "***REDACTED***".to_owned();
        }
    }
}

pub async fn update_settings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let mut settings = state.settings.write().await;
    if let Some(mut env) = req.env {
        // Build merged env: start from incoming, restore redacted from existing
        for (key, value) in env.iter_mut() {
            if *value == "***REDACTED***"
                && let Some(existing) = settings.env.get(key)
            {
                *value = existing.clone();
            }
        }
        env.retain(|_, v| v != "***REDACTED***");

        let api_key = env
            .get("OPENCODE_MEM_API_KEY")
            .or_else(|| env.get("ANTIGRAVITY_API_KEY"))
            .or_else(|| env.get("OPENAI_API_KEY"))
            .cloned();
        let base_url = env
            .get("OPENCODE_MEM_API_URL")
            .or_else(|| env.get("ANTIGRAVITY_BASE_URL"))
            .or_else(|| env.get("OPENAI_BASE_URL"))
            .cloned();
        let model = env.get("OPENCODE_MEM_MODEL").cloned();

        state
            .observation_service
            .update_llm_config(api_key, base_url, model);

        settings.env = env;
    }
    let mut response_settings = settings.clone();
    redact_sensitive_env(&mut response_settings.env);
    Ok(Json(SettingsResponse {
        settings: response_settings,
    }))
}

pub async fn get_mcp_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<McpStatusResponse>, ApiError> {
    let settings = state.settings.read().await;
    Ok(Json(McpStatusResponse {
        enabled: settings.mcp_enabled,
    }))
}

pub async fn toggle_mcp(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ToggleMcpRequest>,
) -> Result<Json<McpStatusResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let mut settings = state.settings.write().await;
    settings.mcp_enabled = req.enabled;
    Ok(Json(McpStatusResponse {
        enabled: settings.mcp_enabled,
    }))
}

pub async fn get_instructions(
    Query(query): Query<InstructionsQuery>,
) -> Result<Json<InstructionsResponse>, ApiError> {
    let content = spawn_blocking(|| {
        let skill_path = Path::new("SKILL.md");
        if skill_path.exists() {
            fs::read_to_string(skill_path).unwrap_or_default()
        } else {
            String::new()
        }
    })
    .await
    .map_err(anyhow::Error::from)?;
    let sections: Vec<String> = content
        .lines()
        .filter(|l| l.starts_with("## "))
        .map(|l| l.trim_start_matches("## ").to_owned())
        .collect();
    let filtered_content = if let Some(section) = query.section {
        extract_section(&content, &section)
    } else {
        content
    };
    Ok(Json(InstructionsResponse {
        sections,
        content: filtered_content,
    }))
}

fn extract_section(content: &str, section: &str) -> String {
    let marker = format!("## {section}");
    let mut in_section = false;
    let mut result = Vec::new();
    for line in content.lines() {
        if line.starts_with("## ") {
            if line == marker {
                in_section = true;
                result.push(line);
            } else if in_section {
                break;
            }
        } else if in_section {
            result.push(line);
        }
    }
    result.join("\n")
}

pub async fn admin_restart(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }

    let tx = state.shutdown_tx.clone();
    tokio::spawn(async move {
        let _ = tx.send(true);
    });

    Ok(Json(AdminResponse {
        success: true,
        message: "Restart initiated (graceful shutdown, then systemd restart)".to_owned(),
    }))
}

pub async fn rebuild_embeddings(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    state
        .search_service
        .clear_embeddings()
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(AdminResponse {
        success: true,
        message: "Embeddings cleared. Run `opencode-mem backfill-embeddings` to regenerate."
            .to_owned(),
    }))
}

pub async fn admin_shutdown(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }

    let tx = state.shutdown_tx.clone();
    tokio::spawn(async move {
        let _ = tx.send(false);
    });

    Ok(Json(AdminResponse {
        success: true,
        message: "Shutdown initiated".to_owned(),
    }))
}
