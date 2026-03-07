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
use std::sync::Arc;

pub async fn get_branch_status(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<BranchStatusResponse>, ApiError> {
    let timeout_duration = std::time::Duration::from_secs(10);

    let branch_fut = async {
        let output = tokio::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        output
    };

    let dirty_fut = async {
        let output = tokio::process::Command::new("git")
            .args(["status", "--porcelain"])
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false);
        output
    };

    let branch = tokio::time::timeout(timeout_duration, branch_fut)
        .await
        .unwrap_or_default();
    let is_dirty = tokio::time::timeout(timeout_duration, dirty_fut)
        .await
        .unwrap_or(false);

    Ok(Json(BranchStatusResponse {
        current_branch: branch,
        is_dirty,
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
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SwitchBranchRequest>,
) -> Result<Json<SwitchBranchResponse>, ApiError> {
    if !super::check_admin_access(&addr, &headers, &state.config) {
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
    let timeout_duration = std::time::Duration::from_secs(15);
    let switch_fut = async {
        tokio::process::Command::new("git")
            .args(["switch", &branch])
            .stdin(std::process::Stdio::null())
            .output()
            .await
    };

    let result = tokio::time::timeout(timeout_duration, switch_fut)
        .await
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("Git switch timed out")))?
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
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UpdateBranchResponse>, ApiError> {
    if !super::check_admin_access(&addr, &headers, &state.config) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let timeout_duration = std::time::Duration::from_secs(30);
    let pull_fut = async {
        tokio::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .stdin(std::process::Stdio::null())
            .output()
            .await
    };

    let result = tokio::time::timeout(timeout_duration, pull_fut)
        .await
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("Git pull timed out")))?
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
