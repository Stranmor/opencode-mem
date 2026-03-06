use super::is_localhost;
use crate::AppState;
use crate::api_error::ApiError;
use crate::api_types::{
    BranchStatusResponse, SwitchBranchRequest, SwitchBranchResponse, UpdateBranchResponse,
};
use axum::{
    Json,
    extract::{ConnectInfo, State},
};
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Arc;
use tokio::task::spawn_blocking;

pub async fn get_branch_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BranchStatusResponse>, ApiError> {
    let settings = state.settings.read().await;
    Ok(Json(BranchStatusResponse {
        current_branch: settings.current_branch.clone(),
        is_dirty: false,
    }))
}

fn validate_branch_name(branch: &str) -> Result<(), &'static str> {
    if branch.is_empty() {
        return Err("Branch name cannot be empty");
    }
    if branch == "." || branch == ".." {
        return Err("Branch name cannot be '.' or '..'");
    }
    if branch.starts_with('-') {
        return Err("Branch name cannot start with dash");
    }
    if !branch
        .chars()
        .all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.')
    {
        return Err("Branch name contains invalid characters");
    }
    Ok(())
}

pub async fn switch_branch(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SwitchBranchRequest>,
) -> Result<Json<SwitchBranchResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    if let Err(msg) = validate_branch_name(&req.branch) {
        return Ok(Json(SwitchBranchResponse {
            success: false,
            branch: req.branch,
            message: msg.to_owned(),
        }));
    }

    let branch = req.branch.clone();
    let result = spawn_blocking(move || Command::new("git").args(["checkout", &branch]).output())
        .await
        .map_err(anyhow::Error::from)?
        .map_err(anyhow::Error::from)?;

    if result.status.success() {
        state
            .settings
            .write()
            .await
            .current_branch
            .clone_from(&req.branch);
        Ok(Json(SwitchBranchResponse {
            success: true,
            branch: req.branch,
            message: "Branch switched successfully".to_owned(),
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(_state): State<Arc<AppState>>,
) -> Result<Json<UpdateBranchResponse>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let result = spawn_blocking(|| Command::new("git").args(["pull", "--ff-only"]).output())
        .await
        .map_err(anyhow::Error::from)?
        .map_err(anyhow::Error::from)?;

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
