use axum::{
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
    Json,
};
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Arc;

use crate::api_types::{
    AdminResponse, BranchStatusResponse, InstructionsQuery, InstructionsResponse,
    McpStatusResponse, SettingsResponse, SwitchBranchRequest, SwitchBranchResponse,
    ToggleMcpRequest, UpdateBranchResponse, UpdateSettingsRequest,
};
use crate::AppState;

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SettingsResponse>, StatusCode> {
    let settings = state.settings.read().await.clone();
    Ok(Json(SettingsResponse { settings }))
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsResponse>, StatusCode> {
    let mut settings = state.settings.write().await;
    if let Some(env) = req.env {
        settings.env = env;
    }
    Ok(Json(SettingsResponse {
        settings: settings.clone(),
    }))
}

pub async fn get_mcp_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<McpStatusResponse>, StatusCode> {
    let settings = state.settings.read().await;
    Ok(Json(McpStatusResponse {
        enabled: settings.mcp_enabled,
    }))
}

pub async fn toggle_mcp(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ToggleMcpRequest>,
) -> Result<Json<McpStatusResponse>, StatusCode> {
    let mut settings = state.settings.write().await;
    settings.mcp_enabled = req.enabled;
    Ok(Json(McpStatusResponse {
        enabled: settings.mcp_enabled,
    }))
}

pub async fn get_branch_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BranchStatusResponse>, StatusCode> {
    let settings = state.settings.read().await;
    Ok(Json(BranchStatusResponse {
        current_branch: settings.current_branch.clone(),
        is_dirty: false,
    }))
}

pub async fn switch_branch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SwitchBranchRequest>,
) -> Result<Json<SwitchBranchResponse>, StatusCode> {
    let branch = req.branch.clone();
    let result = tokio::task::spawn_blocking(move || {
        Command::new("git").args(["checkout", &branch]).output()
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.status.success() {
        let mut settings = state.settings.write().await;
        settings.current_branch = req.branch.clone();
        Ok(Json(SwitchBranchResponse {
            success: true,
            branch: req.branch,
            message: "Branch switched successfully".to_string(),
        }))
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Ok(Json(SwitchBranchResponse {
            success: false,
            branch: req.branch,
            message: format!("Failed to switch branch: {}", stderr.trim()),
        }))
    }
}

pub async fn update_branch(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<UpdateBranchResponse>, StatusCode> {
    let result =
        tokio::task::spawn_blocking(|| Command::new("git").args(["pull", "--ff-only"]).output())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.status.success() {
        let stdout = String::from_utf8_lossy(&result.stdout);
        Ok(Json(UpdateBranchResponse {
            success: true,
            message: format!("Branch updated: {}", stdout.trim()),
        }))
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Ok(Json(UpdateBranchResponse {
            success: false,
            message: format!("Failed to update: {}", stderr.trim()),
        }))
    }
}

pub async fn get_instructions(
    Query(query): Query<InstructionsQuery>,
) -> Result<Json<InstructionsResponse>, StatusCode> {
    let content = tokio::task::spawn_blocking(|| {
        let skill_path = std::path::Path::new("SKILL.md");
        if skill_path.exists() {
            std::fs::read_to_string(skill_path).unwrap_or_default()
        } else {
            String::new()
        }
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let sections: Vec<String> = content
        .lines()
        .filter(|l| l.starts_with("## "))
        .map(|l| l.trim_start_matches("## ").to_string())
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
    let marker = format!("## {}", section);
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

fn is_localhost(addr: &SocketAddr) -> bool {
    addr.ip().is_loopback()
}

pub async fn admin_restart(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<AdminResponse>, StatusCode> {
    if !is_localhost(&addr) {
        return Err(StatusCode::FORBIDDEN);
    }
    // Restart requires external process manager (systemd).
    // Exit with code 0 - systemd with Restart=always will restart the service.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    Ok(Json(AdminResponse {
        success: true,
        message: "Restart initiated (process will exit, systemd will restart)".to_string(),
    }))
}

pub async fn admin_shutdown(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<AdminResponse>, StatusCode> {
    if !is_localhost(&addr) {
        return Err(StatusCode::FORBIDDEN);
    }
    // Graceful shutdown: exit with code 0 after short delay to send response
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    Ok(Json(AdminResponse {
        success: true,
        message: "Shutdown initiated".to_string(),
    }))
}
