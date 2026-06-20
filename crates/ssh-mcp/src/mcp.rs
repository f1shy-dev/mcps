use std::{collections::BTreeMap, sync::Arc};

use axum::{Json, extract::State, http::HeaderMap, response::Response};
use mcp_shared::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, PROTOCOL_VERSION, ToolDefinition,
    handle_streamable_http, invalid_params, method_not_found, parse_tool_input,
    response_from_result, tool_definition, tool_result,
};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    config::{Config, TargetConfig},
    ssh::{SshRunInput, run_ssh_command},
};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    targets: Arc<BTreeMap<String, TargetConfig>>,
}

impl AppState {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            targets: Arc::new(config.targets_by_name()),
            config,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct EmptyInput {}

#[derive(Debug, Serialize)]
struct TargetInfo {
    name: String,
    host: String,
    port: u16,
    user: String,
}

#[derive(Debug, Serialize)]
struct TargetsResult {
    targets: Vec<TargetInfo>,
}

pub async fn handle_mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let allowed_origins = state.config.server.allowed_origins.clone();
    handle_streamable_http(headers, message, &allowed_origins, |request| {
        handle_request(state, request)
    })
    .await
}

async fn handle_request(state: AppState, request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();
    let result = match request.method.as_str() {
        "initialize" => Ok(initialize_result(&state)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(&state, request.params).await,
        method => Err(method_not_found(method)),
    };
    response_from_result(id, result)
}

fn initialize_result(state: &AppState) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": state.config.server.name,
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": state.config.server.instructions
    })
}

fn tools() -> Vec<ToolDefinition> {
    vec![
        tool_definition::<EmptyInput>(
            "ssh_targets",
            "List configured SSH targets without exposing credentials.",
        ),
        tool_definition::<SshRunInput>(
            "ssh_run",
            "Run a non-interactive command on a named SSH target.",
        ),
    ]
}

async fn call_tool(state: &AppState, params: Value) -> Result<Value, JsonRpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("tools/call requires string params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match name {
        "ssh_targets" => Ok(tool_result(targets_result(state), false)),
        "ssh_run" => {
            let input: SshRunInput = parse_tool_input("ssh_run", arguments)?;
            match run_ssh_command(&state.config.limits, &state.targets, input).await {
                Ok(output) => Ok(tool_result(output, false)),
                Err(error) => Ok(tool_result(
                    json!({
                        "ok": false,
                        "error": {
                            "code": error.code(),
                            "message": error.to_string()
                        }
                    }),
                    true,
                )),
            }
        }
        other => Err(invalid_params(format!("unknown tool: {other}"))),
    }
}

fn targets_result(state: &AppState) -> TargetsResult {
    let targets = state
        .targets
        .values()
        .map(|target| TargetInfo {
            name: target.name.clone(),
            host: target.host.clone(),
            port: target.port,
            user: target.user.clone(),
        })
        .collect();
    TargetsResult { targets }
}
