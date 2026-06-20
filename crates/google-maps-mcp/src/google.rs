use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::Result;
use reqwest::Method;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    config::Config,
    store::{CacheInfo, Store, UsageEstimate},
};

#[derive(Clone)]
pub struct GoogleClient {
    config: Arc<Config>,
    store: Arc<Store>,
    http: reqwest::Client,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("{message}")]
    Refused {
        code: &'static str,
        message: String,
        usage: Option<UsageEstimate>,
    },
    #[error("google api error: {0}")]
    Google(String),
    #[error("request error: {0}")]
    Request(String),
}

#[derive(Debug, Serialize)]
pub struct ToolEnvelope {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageEstimate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug)]
pub struct CallSpec {
    pub tool_name: String,
    pub module: &'static str,
    pub sku: &'static str,
    pub units: u32,
    pub unit_price_usd: f64,
    pub cache_ttl_seconds: i64,
}

enum GoogleCredential {
    ApiKey(String),
    OAuthToken(String),
}

impl GoogleClient {
    pub fn new(config: Arc<Config>, store: Arc<Store>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.limits.request_timeout_seconds))
            .user_agent("google-maps-mcp/0.1")
            .build()?;
        Ok(Self {
            config,
            store,
            http,
        })
    }

    pub fn usage_status(&self) -> ToolEnvelope {
        match self
            .store
            .usage_status(self.config.budget.monthly_budget_usd)
        {
            Ok(data) => ok(data, None, None),
            Err(error) => err("USAGE_LEDGER_UNAVAILABLE", error.to_string(), false, None),
        }
    }

    pub fn estimate_tool(&self, tool_name: &str, units: u32) -> ToolEnvelope {
        let Some(spec) = spec_for_tool(tool_name, units.max(1)) else {
            return err(
                "UNKNOWN_TOOL_OR_SKU",
                format!("no pricing estimate configured for {tool_name}"),
                false,
                None,
            );
        };
        match self.estimate(&spec, true) {
            Ok(usage) => ok(json!({ "notes": [], "estimate": usage }), Some(usage), None),
            Err(error) => self.error_envelope(error),
        }
    }

    pub async fn get(
        &self,
        spec: CallSpec,
        endpoint: &str,
        query: BTreeMap<String, String>,
        dry_run: bool,
    ) -> ToolEnvelope {
        self.request(Method::GET, spec, endpoint, query, None, dry_run)
            .await
    }

    pub async fn post(
        &self,
        spec: CallSpec,
        endpoint: &str,
        query: BTreeMap<String, String>,
        body: Value,
        dry_run: bool,
    ) -> ToolEnvelope {
        self.request(Method::POST, spec, endpoint, query, Some(body), dry_run)
            .await
    }

    pub async fn get_image_url(
        &self,
        spec: CallSpec,
        endpoint: &str,
        query: BTreeMap<String, String>,
        dry_run: bool,
    ) -> ToolEnvelope {
        let cache_key = cache_key_for_request(&spec, endpoint, &query, &Value::Null);
        if let Err(error) = self.check_module(&spec) {
            return self.error_envelope(error);
        }
        match self.estimate(&spec, dry_run) {
            Ok(usage) => ok(
                json!({
                    "url": build_url_without_key(endpoint, &query),
                    "dry_run": dry_run,
                    "billable_when_fetched": true,
                    "usage_recorded": false,
                }),
                Some(usage),
                Some(CacheInfo {
                    hit: false,
                    ttl_seconds: None,
                    cache_key: Some(cache_key),
                }),
            ),
            Err(error) => self.error_envelope(error),
        }
    }

    async fn request(
        &self,
        method: Method,
        spec: CallSpec,
        endpoint: &str,
        query: BTreeMap<String, String>,
        body: Option<Value>,
        dry_run: bool,
    ) -> ToolEnvelope {
        let cache_body = body.clone().unwrap_or(Value::Null);
        let cache_key = cache_key_for_request(&spec, endpoint, &query, &cache_body);
        match self.check_module(&spec) {
            Ok(()) => {}
            Err(error) => return self.error_envelope(error),
        }
        if dry_run {
            return match self.estimate(&spec, true) {
                Ok(usage) => ok(
                    json!({
                        "dry_run": true,
                        "endpoint": endpoint,
                        "method": method.as_str(),
                    }),
                    Some(usage),
                    Some(CacheInfo {
                        hit: false,
                        ttl_seconds: None,
                        cache_key: Some(cache_key),
                    }),
                ),
                Err(error) => self.error_envelope(error),
            };
        }

        if let Ok(Some(entry)) = self.store.cache_get(&cache_key) {
            let usage = self
                .store
                .estimate(
                    spec.sku,
                    0,
                    spec.unit_price_usd,
                    self.config.budget.monthly_budget_usd,
                    false,
                )
                .ok();
            if let Some(usage) = &usage {
                let _ =
                    self.store
                        .record_usage(&spec.tool_name, usage, true, &cache_key, "ok", None);
            }
            return ok(
                entry.value,
                usage,
                Some(CacheInfo {
                    hit: true,
                    ttl_seconds: entry.ttl_seconds,
                    cache_key: Some(cache_key),
                }),
            );
        }

        let credential = match self.credential_for(endpoint) {
            Ok(credential) => credential,
            Err(error) => return self.error_envelope(error),
        };

        let Some(credential) = credential else {
            return err(
                "CONFIG_MISSING_API_KEY",
                format!("{} is not set", self.config.provider.api_key_env),
                false,
                None,
            );
        };

        match self.reserve(&spec, &cache_key) {
            Ok(usage) => {
                if !usage.allowed {
                    return err(
                        "MONTHLY_BUDGET_EXCEEDED",
                        "projected call would exceed local monthly budget",
                        false,
                        Some(usage),
                    );
                }

                let result = self
                    .send_google(method, endpoint, query, body, credential)
                    .await;
                match result {
                    Ok(value) => {
                        let _ = self
                            .store
                            .cache_put(&cache_key, spec.cache_ttl_seconds, &value);
                        ok(
                            value,
                            Some(usage),
                            Some(CacheInfo {
                                hit: false,
                                ttl_seconds: Some(spec.cache_ttl_seconds),
                                cache_key: Some(cache_key),
                            }),
                        )
                    }
                    Err(error) => {
                        let code = match &error {
                            ToolError::Google(_) => "GOOGLE_API_ERROR",
                            ToolError::Request(_) => "REQUEST_ERROR",
                            ToolError::Refused { code, .. } => code,
                        };
                        let _ = self.store.record_usage(
                            &spec.tool_name,
                            &usage,
                            false,
                            &cache_key,
                            "error",
                            Some(code),
                        );
                        self.error_envelope(error)
                    }
                }
            }
            Err(error) => self.error_envelope(error),
        }
    }

    fn check_module(&self, spec: &CallSpec) -> Result<(), ToolError> {
        if !self.config.module_enabled(spec.module) {
            return Err(ToolError::Refused {
                code: "CONFIG_MODULE_DISABLED",
                message: format!("module is disabled: {}", spec.module),
                usage: None,
            });
        }
        Ok(())
    }

    fn estimate(&self, spec: &CallSpec, dry_run: bool) -> Result<UsageEstimate, ToolError> {
        self.store
            .estimate(
                spec.sku,
                spec.units,
                spec.unit_price_usd,
                self.config.budget.monthly_budget_usd,
                dry_run,
            )
            .map_err(|error| ToolError::Refused {
                code: "USAGE_LEDGER_UNAVAILABLE",
                message: error.to_string(),
                usage: None,
            })
    }

    fn reserve(&self, spec: &CallSpec, cache_key: &str) -> Result<UsageEstimate, ToolError> {
        self.store
            .reserve_usage(
                &spec.tool_name,
                spec.sku,
                spec.units,
                spec.unit_price_usd,
                self.config.budget.monthly_budget_usd,
                cache_key,
            )
            .map_err(|error| ToolError::Refused {
                code: "USAGE_LEDGER_UNAVAILABLE",
                message: error.to_string(),
                usage: None,
            })
    }

    fn credential_for(&self, endpoint: &str) -> Result<Option<GoogleCredential>, ToolError> {
        if endpoint.starts_with("https://routeoptimization.googleapis.com/") {
            return self
                .config
                .oauth_token()
                .map(|token| Some(GoogleCredential::OAuthToken(token)))
                .ok_or_else(|| ToolError::Refused {
                    code: "CONFIG_MISSING_OAUTH_TOKEN",
                    message: format!("{} is not set", self.config.provider.oauth_token_env),
                    usage: None,
                });
        }
        Ok(self.config.api_key().map(GoogleCredential::ApiKey))
    }

    async fn send_google(
        &self,
        method: Method,
        endpoint: &str,
        mut query: BTreeMap<String, String>,
        body: Option<Value>,
        credential: GoogleCredential,
    ) -> Result<Value, ToolError> {
        let uses_header_key = endpoint.starts_with("https://places.googleapis.com/")
            || endpoint.starts_with("https://routes.googleapis.com/");
        if !uses_header_key && let GoogleCredential::ApiKey(api_key) = &credential {
            query.insert("key".to_string(), api_key.to_string());
        }
        let url = build_url(endpoint, &query);
        let mut request = self.http.request(method, &url);
        match credential {
            GoogleCredential::ApiKey(api_key) if uses_header_key => {
                request = request.header("X-Goog-Api-Key", api_key);
            }
            GoogleCredential::OAuthToken(token) => {
                request = request.bearer_auth(token);
            }
            _ => {}
        }
        if let Some(field_mask) = field_mask_for(endpoint) {
            request = request.header("X-Goog-FieldMask", field_mask);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ToolError::Request(redact_key(&error.to_string())))?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|len| len > self.config.limits.max_raw_response_bytes as u64)
        {
            return Err(ToolError::Request(format!(
                "google response exceeded max_raw_response_bytes ({})",
                self.config.limits.max_raw_response_bytes
            )));
        }
        let text = response
            .bytes()
            .await
            .map_err(|error| ToolError::Request(redact_key(&error.to_string())))?;
        if text.len() > self.config.limits.max_raw_response_bytes {
            return Err(ToolError::Request(format!(
                "google response exceeded max_raw_response_bytes ({})",
                self.config.limits.max_raw_response_bytes
            )));
        }
        let text = String::from_utf8_lossy(&text);
        let value = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| {
            json!({
                "text": text,
            })
        });
        if !status.is_success() || google_payload_is_error(&value) {
            return Err(ToolError::Google(redact_key(&value.to_string())));
        }
        Ok(value)
    }

    fn error_envelope(&self, error: ToolError) -> ToolEnvelope {
        match error {
            ToolError::Refused {
                code,
                message,
                usage,
            } => err(code, message, false, usage),
            ToolError::Google(message) => err("GOOGLE_API_ERROR", message, false, None),
            ToolError::Request(message) => err("REQUEST_ERROR", message, true, None),
        }
    }
}

pub fn ok(data: Value, usage: Option<UsageEstimate>, cache: Option<CacheInfo>) -> ToolEnvelope {
    ToolEnvelope {
        ok: true,
        data: Some(data),
        usage,
        cache,
        error: None,
    }
}

pub fn err(
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
    usage: Option<UsageEstimate>,
) -> ToolEnvelope {
    ToolEnvelope {
        ok: false,
        data: None,
        usage,
        cache: None,
        error: Some(ErrorBody {
            code: code.into(),
            message: message.into(),
            retryable,
        }),
    }
}

pub fn spec_for_tool(tool_name: &str, units: u32) -> Option<CallSpec> {
    let (module, sku, price, ttl) = match tool_name {
        "gmaps_geocode" | "gmaps_reverse_geocode" => {
            ("geocoding", "Geocoding", 0.005, 30 * 24 * 3600)
        }
        "gmaps_find_place" | "gmaps_place_details" => ("places", "Places", 0.017, 24 * 3600),
        "gmaps_autocomplete" => ("autocomplete", "Autocomplete", 0.00283, 24 * 3600),
        "gmaps_timezone" => ("timezone", "Time Zone", 0.005, 30 * 24 * 3600),
        "gmaps_route" => ("routes", "Routes", 0.005, 5 * 60),
        "gmaps_route_matrix" => ("route_matrix", "Route Matrix", 0.005, 5 * 60),
        "gmaps_static_map" => ("static_maps", "Static Maps", 0.002, 7 * 24 * 3600),
        "gmaps_streetview" => ("streetview", "Static Street View", 0.007, 7 * 24 * 3600),
        "gmaps_elevation" => ("elevation", "Elevation", 0.005, 30 * 24 * 3600),
        "gmaps_weather" => ("weather", "Weather", 0.005, 15 * 60),
        "gmaps_air_quality" => ("air_quality", "Air Quality", 0.005, 15 * 60),
        "gmaps_pollen" => ("pollen", "Pollen", 0.005, 60 * 60),
        "gmaps_address_validation" => {
            ("address_validation", "Address Validation", 0.017, 24 * 3600)
        }
        "gmaps_roads" => ("roads", "Roads", 0.01, 24 * 3600),
        "gmaps_route_optimization" => ("route_optimization", "Route Optimization", 0.05, 5 * 60),
        "gmaps_solar" => ("solar", "Solar", 0.01, 24 * 3600),
        _ => return None,
    };
    Some(CallSpec {
        tool_name: tool_name.to_string(),
        module,
        sku,
        units,
        unit_price_usd: price,
        cache_ttl_seconds: ttl,
    })
}

pub fn cache_key_for_request(
    spec: &CallSpec,
    endpoint: &str,
    query: &BTreeMap<String, String>,
    body: &Value,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(spec.tool_name.as_bytes());
    hasher.update(spec.sku.as_bytes());
    hasher.update(spec.units.to_be_bytes());
    hasher.update(endpoint.as_bytes());
    hasher.update(serde_json::to_vec(query).unwrap_or_default());
    hasher.update(serde_json::to_vec(body).unwrap_or_default());
    format!("{:x}", hasher.finalize())
}

pub fn build_url(endpoint: &str, query: &BTreeMap<String, String>) -> String {
    if query.is_empty() {
        return endpoint.to_string();
    }
    let params = query
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    format!("{endpoint}{separator}{params}")
}

fn build_url_without_key(endpoint: &str, query: &BTreeMap<String, String>) -> String {
    build_url(endpoint, query)
}

fn field_mask_for(endpoint: &str) -> Option<&'static str> {
    if endpoint.contains("places:searchText") {
        Some(
            "places.id,places.displayName,places.formattedAddress,places.location,places.types,places.rating,places.userRatingCount,places.googleMapsUri",
        )
    } else if endpoint.contains("places:autocomplete") {
        Some(
            "suggestions.placePrediction.placeId,suggestions.placePrediction.text,suggestions.placePrediction.types",
        )
    } else if endpoint.contains("/v1/places/") {
        Some(
            "id,displayName,formattedAddress,location,types,rating,userRatingCount,googleMapsUri,websiteUri,nationalPhoneNumber,internationalPhoneNumber,regularOpeningHours,businessStatus,priceLevel",
        )
    } else if endpoint.contains("directions/v2:computeRoutes") {
        Some(
            "routes.duration,routes.distanceMeters,routes.polyline.encodedPolyline,routes.description,routes.legs",
        )
    } else {
        None
    }
}

fn google_payload_is_error(value: &Value) -> bool {
    value.get("error").is_some()
        || value
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| !matches!(status, "OK" | "ZERO_RESULTS" | "NOT_FOUND"))
}

pub fn redact_key(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("key=") {
        let (prefix, suffix) = rest.split_at(index);
        redacted.push_str(prefix);
        redacted.push_str("key=REDACTED");
        let value = &suffix[4..];
        let end = value
            .find(|ch: char| {
                matches!(
                    ch,
                    '&' | ' ' | '\t' | '\r' | '\n' | '"' | '\'' | ')' | '<' | '>'
                )
            })
            .unwrap_or(value.len());
        rest = &value[end..];
    }
    redacted.push_str(rest);
    redacted
}

#[cfg(test)]
mod tests {
    use super::redact_key;

    #[test]
    fn redacts_embedded_query_keys() {
        let text =
            "error sending request for url (https://x.test/maps?center=a&key=leakme123&size=1)";
        let redacted = redact_key(text);
        assert!(!redacted.contains("leakme123"));
        assert!(redacted.contains("key=REDACTED"));
        assert!(redacted.contains("size=1"));
    }
}
