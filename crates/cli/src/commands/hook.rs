use anyhow::Result;
use clap::Subcommand;
use opencode_mem_core::{ObservationHookRequest, SessionInitHookRequest, SummarizeHookRequest};
use std::io::{IsTerminal, Read};

#[derive(Subcommand)]
pub enum HookCommands {
    Context {
        #[arg(short, long)]
        project: Option<String>,
        #[arg(short, long, default_value = "50")]
        limit: usize,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
    },
    SessionInit {
        #[arg(long)]
        content_session_id: Option<String>,
        #[arg(short, long)]
        project: Option<String>,
        #[arg(long)]
        user_prompt: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
    },
    Observe {
        #[arg(short, long)]
        tool: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(short, long)]
        project: Option<String>,
        #[arg(short, long, help = "Tool input arguments as JSON string")]
        input: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
    },
    Summarize {
        #[arg(long)]
        content_session_id: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
    },
}

fn get_project_from_stdin() -> Option<String> {
    if std::io::stdin().is_terminal() {
        return None;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    let json: serde_json::Value = serde_json::from_str(&input).ok()?;
    json.get("project")
        .or_else(|| json.get("project_path"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn build_session_init_request(
    content_session_id: Option<String>,
    project: Option<String>,
    user_prompt: Option<String>,
) -> Result<SessionInitHookRequest> {
    let session_id = content_session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    Ok(SessionInitHookRequest {
        content_session_id: session_id,
        project,
        user_prompt,
    })
}

fn build_observation_request(
    tool: Option<String>,
    session_id: Option<String>,
    project: Option<String>,
    input_json: Option<String>,
) -> Result<ObservationHookRequest> {
    let mut output_str = String::new();
    if !std::io::stdin().is_terminal() {
        std::io::stdin().read_to_string(&mut output_str)?;
    }
    let tool_name = tool.unwrap_or_else(|| "unknown".to_string());
    let input: Option<serde_json::Value> = input_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());
    Ok(ObservationHookRequest {
        tool: tool_name,
        session_id,
        call_id: None,
        project,
        input,
        output: output_str,
    })
}

fn build_summarize_request(
    content_session_id: Option<String>,
    session_id: Option<String>,
) -> Result<SummarizeHookRequest> {
    Ok(SummarizeHookRequest {
        content_session_id,
        session_id,
    })
}

pub async fn run(cmd: HookCommands) -> Result<()> {
    let client = reqwest::Client::new();

    match cmd {
        HookCommands::Context {
            project,
            limit,
            endpoint,
        } => {
            let project = project.or_else(get_project_from_stdin).ok_or_else(|| {
                anyhow::anyhow!("Project required: use --project or pipe JSON with 'project' field")
            })?;
            let url = format!("{}/context/inject", endpoint);
            let resp = client
                .get(&url)
                .query(&[("project", &project), ("limit", &limit.to_string())])
                .send()
                .await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        HookCommands::SessionInit {
            content_session_id,
            project,
            user_prompt,
            endpoint,
        } => {
            let req = build_session_init_request(content_session_id, project, user_prompt)?;
            let url = format!("{}/api/sessions/init", endpoint);
            let resp = client.post(&url).json(&req).send().await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        HookCommands::Observe {
            tool,
            session_id,
            project,
            input,
            endpoint,
        } => {
            let req = build_observation_request(tool, session_id, project, input)?;
            let url = format!("{}/observe", endpoint);
            let resp = client.post(&url).json(&req).send().await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        HookCommands::Summarize {
            content_session_id,
            session_id,
            endpoint,
        } => {
            let req = build_summarize_request(content_session_id, session_id)?;
            let url = format!("{}/api/sessions/summarize", endpoint);
            let resp = client.post(&url).json(&req).send().await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
    }

    Ok(())
}
