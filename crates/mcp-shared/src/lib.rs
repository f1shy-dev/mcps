use std::future::Future;

use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn method_not_allowed() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}

pub async fn handle_streamable_http<F, Fut>(
    headers: HeaderMap,
    message: Value,
    allowed_origins: &[String],
    dispatch: F,
) -> Response
where
    F: FnOnce(JsonRpcRequest) -> Fut,
    Fut: Future<Output = JsonRpcResponse>,
{
    if !origin_allowed(allowed_origins, &headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            None,
            -32000,
            "origin is not allowed",
            None,
        );
    }

    if let Err(error) = validate_protocol_version(&headers) {
        return error_response(
            StatusCode::BAD_REQUEST,
            json_rpc_id(&message),
            -32000,
            error,
            None,
        );
    }

    if is_response(&message) {
        return StatusCode::ACCEPTED.into_response();
    }

    let request = match serde_json::from_value::<JsonRpcRequest>(message.clone()) {
        Ok(request) => request,
        Err(error) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                json_rpc_id(&message),
                -32600,
                "invalid JSON-RPC request",
                Some(json!({ "error": error.to_string() })),
            );
        }
    };

    if let Some(error) = validate_json_rpc_request(&request) {
        return error_response(
            StatusCode::BAD_REQUEST,
            request.id.clone(),
            -32600,
            error,
            None,
        );
    }

    if request.id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }

    let response = dispatch(request).await;
    json_response(StatusCode::OK, &response)
}

pub fn response_from_result(
    id: Option<Value>,
    result: Result<Value, JsonRpcError>,
) -> JsonRpcResponse {
    match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(value),
            error: None,
        },
        Err(error) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        },
    }
}

pub fn method_not_found(method: impl Into<String>) -> JsonRpcError {
    JsonRpcError {
        code: -32601,
        message: format!("method not found: {}", method.into()),
        data: None,
    }
}

pub fn invalid_params(message: impl Into<String>) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: message.into(),
        data: None,
    }
}

pub fn tool_result<T: Serialize>(data: T, is_error: bool) -> Value {
    let structured = serde_json::to_value(data).unwrap_or_else(|_| json!({}));
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "structuredContent": structured,
        "isError": is_error
    })
}

pub fn schema_value<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).unwrap_or_else(|_| json!({ "type": "object" }))
}

fn origin_allowed(allowed_origins: &[String], headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    allowed_origins.iter().any(|allowed| allowed == origin)
}

fn validate_protocol_version(headers: &HeaderMap) -> Result<(), String> {
    let Some(version) = headers.get("MCP-Protocol-Version") else {
        return Ok(());
    };
    let version = version
        .to_str()
        .map_err(|_| "invalid MCP-Protocol-Version header".to_string())?;
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(format!("unsupported MCP protocol version: {version}"))
    }
}

fn validate_json_rpc_request(request: &JsonRpcRequest) -> Option<String> {
    match request.jsonrpc.as_deref() {
        Some("2.0") => {}
        _ => return Some("JSON-RPC version must be 2.0".to_string()),
    }
    if matches!(request.id, Some(Value::Null)) {
        return Some("JSON-RPC request id must be a string or number".to_string());
    }
    if request.method.is_empty() {
        return Some("JSON-RPC method must not be empty".to_string());
    }
    None
}

fn is_response(message: &Value) -> bool {
    message.get("method").is_none()
        && message.get("id").is_some()
        && (message.get("result").is_some() || message.get("error").is_some())
}

fn json_rpc_id(message: &Value) -> Option<Value> {
    match message.get("id") {
        Some(Value::String(_)) | Some(Value::Number(_)) => message.get("id").cloned(),
        _ => None,
    }
}

fn error_response(
    status: StatusCode,
    id: Option<Value>,
    code: i64,
    message: impl Into<String>,
    data: Option<Value>,
) -> Response {
    json_response(
        status,
        &JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data,
            }),
        },
    )
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    response
}
