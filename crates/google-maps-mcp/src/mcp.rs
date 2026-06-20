use std::{collections::BTreeMap, sync::Arc};

use axum::{Json, extract::State, http::HeaderMap, response::Response};
use mcp_shared::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, PROTOCOL_VERSION, handle_streamable_http,
    invalid_params, method_not_found, response_from_result, schema_value, tool_result,
};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    config::Config,
    google::{GoogleClient, ToolEnvelope, err, spec_for_tool},
};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    google: Arc<GoogleClient>,
}

impl AppState {
    pub fn new(config: Arc<Config>, google: Arc<GoogleClient>) -> Self {
        Self { config, google }
    }
}

#[derive(Debug, Serialize)]
struct Tool {
    name: &'static str,
    description: &'static str,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
struct LooseInput {
    #[serde(flatten)]
    args: BTreeMap<String, Value>,
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

fn tools() -> Vec<Tool> {
    [
        ("gmaps_usage_status", "Report local Google Maps usage ledger and remaining monthly budget without calling Google."),
        ("gmaps_estimate_call", "Estimate cost and budget allowance for a planned Google Maps tool call without calling Google."),
        ("gmaps_geocode", "Convert an address into coordinates and Google place metadata."),
        ("gmaps_reverse_geocode", "Convert latitude/longitude into addresses and place metadata."),
        ("gmaps_find_place", "Find places, businesses, POIs, or landmarks by text query."),
        ("gmaps_place_details", "Fetch controlled details for a Google place ID."),
        ("gmaps_autocomplete", "Return place/address autocomplete predictions, optionally using a session token."),
        ("gmaps_timezone", "Get timezone information for a location."),
        ("gmaps_route", "Compute routes between locations using Google Routes API."),
        ("gmaps_route_matrix", "Compare travel times/distances between many origins and destinations."),
        ("gmaps_static_map", "Build a Google Static Maps URL for a location, markers, or route."),
        ("gmaps_streetview", "Build a Street View image URL or fetch Street View metadata."),
        ("gmaps_elevation", "Get elevation for points or sampled paths."),
        ("gmaps_weather", "Get current weather or forecast for a location."),
        ("gmaps_air_quality", "Get air quality conditions for a location."),
        ("gmaps_pollen", "Get pollen forecast data for a location."),
        ("gmaps_address_validation", "Validate and normalize a postal address."),
        ("gmaps_roads", "Use Roads API for nearest roads, snap-to-roads, or speed limits."),
        ("gmaps_route_optimization", "Run a Google Route Optimization request for small routing problems."),
        ("gmaps_solar", "Query Google Solar building insights near a location."),
    ]
    .into_iter()
    .map(|(name, description)| Tool {
        name,
        description,
        input_schema: schema_value::<LooseInput>(),
    })
    .collect()
}

async fn call_tool(state: &AppState, params: Value) -> Result<Value, JsonRpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("tools/call requires string params.name"))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let envelope = match name {
        "gmaps_usage_status" => state.google.usage_status(),
        "gmaps_estimate_call" => {
            let tool_name = str_arg(&args, "tool_name").unwrap_or_default();
            let units = u32_arg(&args, "planned_units").unwrap_or(1);
            state.google.estimate_tool(tool_name, units)
        }
        "gmaps_geocode" => geocode(state, args).await,
        "gmaps_reverse_geocode" => reverse_geocode(state, args).await,
        "gmaps_find_place" => find_place(state, args).await,
        "gmaps_place_details" => place_details(state, args).await,
        "gmaps_autocomplete" => autocomplete(state, args).await,
        "gmaps_timezone" => timezone(state, args).await,
        "gmaps_route" => route(state, args).await,
        "gmaps_route_matrix" => route_matrix(state, args).await,
        "gmaps_static_map" => static_map(state, args).await,
        "gmaps_streetview" => streetview(state, args).await,
        "gmaps_elevation" => elevation(state, args).await,
        "gmaps_weather" => weather(state, args).await,
        "gmaps_air_quality" => air_quality(state, args).await,
        "gmaps_pollen" => pollen(state, args).await,
        "gmaps_address_validation" => address_validation(state, args).await,
        "gmaps_roads" => roads(state, args).await,
        "gmaps_route_optimization" => route_optimization(state, args).await,
        "gmaps_solar" => solar(state, args).await,
        other => return Err(invalid_params(format!("unknown tool: {other}"))),
    };
    Ok(tool_result(&envelope, !envelope.ok))
}

async fn geocode(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(address) = str_arg(&args, "address") else {
        return bad_input("address is required");
    };
    let mut query = common_query(state);
    query.insert("address".to_string(), address.to_string());
    optional_string_query(&args, &mut query, "region_code", "region");
    optional_string_query(&args, &mut query, "language_code", "language");
    state
        .google
        .get(
            spec("gmaps_geocode", 1),
            "https://maps.googleapis.com/maps/api/geocode/json",
            query,
            dry_run(&args),
        )
        .await
}

async fn reverse_geocode(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(location) = lat_lng_query(args.get("location").unwrap_or(&args)) else {
        return bad_input("location with lat/lng is required");
    };
    let mut query = common_query(state);
    query.insert("latlng".to_string(), location);
    optional_join_query(&args, &mut query, "result_type", "result_type");
    optional_join_query(&args, &mut query, "location_type", "location_type");
    state
        .google
        .get(
            spec("gmaps_reverse_geocode", 1),
            "https://maps.googleapis.com/maps/api/geocode/json",
            query,
            dry_run(&args),
        )
        .await
}

async fn find_place(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(query_text) = str_arg(&args, "query") else {
        return bad_input("query is required");
    };
    let max =
        u32_arg(&args, "max_results").unwrap_or(state.config.limits.max_places_results_default);
    if max > state.config.limits.max_places_results_hard {
        return hard_limit(
            "max_results",
            max,
            state.config.limits.max_places_results_hard,
        );
    }
    let mut body = json!({
        "textQuery": query_text,
        "maxResultCount": max,
        "languageCode": state.config.provider.language_code,
        "regionCode": state.config.provider.region_code,
    });
    if let Some(near) = args.get("near").and_then(lat_lng_value) {
        body["locationBias"] = json!({
            "circle": {
                "center": near,
                "radius": u32_arg(&args, "radius_meters").unwrap_or(5000)
            }
        });
    }
    state
        .google
        .post(
            spec("gmaps_find_place", 1),
            "https://places.googleapis.com/v1/places:searchText",
            BTreeMap::new(),
            body,
            dry_run(&args),
        )
        .await
}

async fn place_details(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(place_id) = str_arg(&args, "place_id") else {
        return bad_input("place_id is required");
    };
    let place_id = place_id.trim_start_matches("places/");
    let endpoint = format!("https://places.googleapis.com/v1/places/{place_id}");
    let mut query = BTreeMap::new();
    optional_string_query(&args, &mut query, "language_code", "languageCode");
    optional_string_query(&args, &mut query, "region_code", "regionCode");
    state
        .google
        .get(
            spec("gmaps_place_details", 1),
            &endpoint,
            query,
            dry_run(&args),
        )
        .await
}

async fn autocomplete(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(input) = str_arg(&args, "input") else {
        return bad_input("input is required");
    };
    let mut body = json!({
        "input": input,
        "languageCode": state.config.provider.language_code,
        "regionCode": state.config.provider.region_code,
    });
    if let Some(token) = str_arg(&args, "session_token") {
        body["sessionToken"] = json!(token);
    }
    if let Some(bias) = args.get("location_bias").and_then(lat_lng_value) {
        body["locationBias"] = json!({ "circle": { "center": bias, "radius": 5000 } });
    }
    state
        .google
        .post(
            spec("gmaps_autocomplete", 1),
            "https://places.googleapis.com/v1/places:autocomplete",
            BTreeMap::new(),
            body,
            dry_run(&args),
        )
        .await
}

async fn timezone(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(location) = location_query(&args, "location") else {
        return bad_input("location is required");
    };
    let mut query = common_query(state);
    query.insert("location".to_string(), location);
    query.insert(
        "timestamp".to_string(),
        str_arg(&args, "timestamp")
            .and_then(|ts| ts.parse::<i64>().ok())
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
            .to_string(),
    );
    state
        .google
        .get(
            spec("gmaps_timezone", 1),
            "https://maps.googleapis.com/maps/api/timezone/json",
            query,
            dry_run(&args),
        )
        .await
}

async fn route(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(origin) = args.get("origin").and_then(waypoint_value) else {
        return bad_input("origin is required");
    };
    let Some(destination) = args.get("destination").and_then(waypoint_value) else {
        return bad_input("destination is required");
    };
    let waypoints = args
        .get("waypoints")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(waypoint_value).collect::<Vec<_>>())
        .unwrap_or_default();
    if waypoints.len() as u32 > state.config.limits.max_waypoints_hard {
        return hard_limit(
            "waypoints",
            waypoints.len() as u32,
            state.config.limits.max_waypoints_hard,
        );
    }
    let mut body = json!({
        "origin": origin,
        "destination": destination,
        "travelMode": str_arg(&args, "travel_mode").unwrap_or("DRIVE"),
        "routingPreference": str_arg(&args, "routing_preference").unwrap_or("TRAFFIC_AWARE"),
        "computeAlternativeRoutes": bool_arg(&args, "compute_alternative_routes").unwrap_or(false),
        "languageCode": state.config.provider.language_code,
        "units": str_arg(&args, "units").unwrap_or(&state.config.provider.units),
    });
    if !waypoints.is_empty() {
        body["intermediates"] = json!(waypoints);
    }
    body["routeModifiers"] = json!({
        "avoidTolls": bool_arg(&args, "avoid_tolls").unwrap_or(false),
        "avoidHighways": bool_arg(&args, "avoid_highways").unwrap_or(false),
        "avoidFerries": bool_arg(&args, "avoid_ferries").unwrap_or(false),
    });
    state
        .google
        .post(
            spec("gmaps_route", 1),
            "https://routes.googleapis.com/directions/v2:computeRoutes",
            BTreeMap::new(),
            body,
            dry_run(&args),
        )
        .await
}

async fn route_matrix(state: &AppState, args: Value) -> ToolEnvelope {
    let origins = location_list(&args, "origins");
    let destinations = location_list(&args, "destinations");
    if origins.is_empty() || destinations.is_empty() {
        return bad_input("origins and destinations are required");
    }
    let elements = (origins.len() * destinations.len()) as u32;
    if elements > state.config.limits.max_route_matrix_elements_hard {
        return hard_limit(
            "route_matrix_elements",
            elements,
            state.config.limits.max_route_matrix_elements_hard,
        );
    }
    let mut query = common_query(state);
    query.insert("origins".to_string(), origins.join("|"));
    query.insert("destinations".to_string(), destinations.join("|"));
    query.insert(
        "mode".to_string(),
        str_arg(&args, "travel_mode")
            .unwrap_or("DRIVING")
            .to_lowercase(),
    );
    state
        .google
        .get(
            spec("gmaps_route_matrix", elements),
            "https://maps.googleapis.com/maps/api/distancematrix/json",
            query,
            dry_run(&args),
        )
        .await
}

async fn static_map(state: &AppState, args: Value) -> ToolEnvelope {
    let width = args
        .get("size")
        .and_then(|v| u32_arg(v, "width"))
        .unwrap_or(640);
    let height = args
        .get("size")
        .and_then(|v| u32_arg(v, "height"))
        .unwrap_or(640);
    if width > state.config.limits.max_static_map_width {
        return hard_limit("width", width, state.config.limits.max_static_map_width);
    }
    if height > state.config.limits.max_static_map_height {
        return hard_limit("height", height, state.config.limits.max_static_map_height);
    }
    let mut query = BTreeMap::new();
    query.insert("size".to_string(), format!("{width}x{height}"));
    if let Some(center) = location_query(&args, "center") {
        query.insert("center".to_string(), center);
    }
    if let Some(zoom) = u32_arg(&args, "zoom") {
        query.insert("zoom".to_string(), zoom.to_string());
    }
    if let Some(map_type) = str_arg(&args, "map_type") {
        query.insert("maptype".to_string(), map_type.to_string());
    }
    state
        .google
        .get_image_url(
            spec("gmaps_static_map", 1),
            "https://maps.googleapis.com/maps/api/staticmap",
            query,
            dry_run(&args),
        )
        .await
}

async fn streetview(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(location) = location_query(&args, "location") else {
        return bad_input("location is required");
    };
    let width = args
        .get("size")
        .and_then(|v| u32_arg(v, "width"))
        .unwrap_or(640);
    let height = args
        .get("size")
        .and_then(|v| u32_arg(v, "height"))
        .unwrap_or(640);
    let mut query = BTreeMap::new();
    query.insert("location".to_string(), location);
    query.insert("size".to_string(), format!("{width}x{height}"));
    optional_string_query(&args, &mut query, "heading", "heading");
    optional_string_query(&args, &mut query, "pitch", "pitch");
    optional_string_query(&args, &mut query, "fov", "fov");
    let endpoint = if bool_arg(&args, "metadata_only").unwrap_or(false) {
        "https://maps.googleapis.com/maps/api/streetview/metadata"
    } else {
        "https://maps.googleapis.com/maps/api/streetview"
    };
    state
        .google
        .get_image_url(spec("gmaps_streetview", 1), endpoint, query, dry_run(&args))
        .await
}

async fn elevation(state: &AppState, args: Value) -> ToolEnvelope {
    let mut query = BTreeMap::new();
    if let Some(locations) = args.get("locations").and_then(Value::as_array) {
        query.insert(
            "locations".to_string(),
            locations
                .iter()
                .filter_map(lat_lng_query)
                .collect::<Vec<_>>()
                .join("|"),
        );
    } else if let Some(path) = args.get("path").and_then(Value::as_array) {
        query.insert(
            "path".to_string(),
            path.iter()
                .filter_map(lat_lng_query)
                .collect::<Vec<_>>()
                .join("|"),
        );
        optional_string_query(&args, &mut query, "samples", "samples");
    } else {
        return bad_input("locations or path is required");
    }
    state
        .google
        .get(
            spec("gmaps_elevation", 1),
            "https://maps.googleapis.com/maps/api/elevation/json",
            query,
            dry_run(&args),
        )
        .await
}

async fn weather(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(location) = args.get("location").and_then(lat_lng_value) else {
        return bad_input("weather currently requires lat_lng location");
    };
    let mode = str_arg(&args, "mode").unwrap_or("current");
    let endpoint = if mode == "forecast" {
        "https://weather.googleapis.com/v1/forecast/hours:lookup"
    } else {
        "https://weather.googleapis.com/v1/currentConditions:lookup"
    };
    let mut query = BTreeMap::new();
    query.insert(
        "location.latitude".to_string(),
        location["latitude"].to_string(),
    );
    query.insert(
        "location.longitude".to_string(),
        location["longitude"].to_string(),
    );
    state
        .google
        .get(spec("gmaps_weather", 1), endpoint, query, dry_run(&args))
        .await
}

async fn air_quality(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(location) = args.get("location").and_then(lat_lng_value) else {
        return bad_input("air quality currently requires lat_lng location");
    };
    let body = json!({
        "location": location,
        "extraComputations": if bool_arg(&args, "include_pollutants").unwrap_or(false) {
            json!(["POLLUTANT_CONCENTRATION"])
        } else {
            json!([])
        }
    });
    state
        .google
        .post(
            spec("gmaps_air_quality", 1),
            "https://airquality.googleapis.com/v1/currentConditions:lookup",
            BTreeMap::new(),
            body,
            dry_run(&args),
        )
        .await
}

async fn pollen(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(location) = args.get("location").and_then(lat_lng_value) else {
        return bad_input("pollen currently requires lat_lng location");
    };
    let mut query = BTreeMap::new();
    query.insert(
        "location.latitude".to_string(),
        location["latitude"].to_string(),
    );
    query.insert(
        "location.longitude".to_string(),
        location["longitude"].to_string(),
    );
    query.insert(
        "days".to_string(),
        u32_arg(&args, "days").unwrap_or(1).to_string(),
    );
    state
        .google
        .get(
            spec("gmaps_pollen", 1),
            "https://pollen.googleapis.com/v1/forecast:lookup",
            query,
            dry_run(&args),
        )
        .await
}

async fn address_validation(state: &AppState, args: Value) -> ToolEnvelope {
    let lines = args
        .get("address_lines")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    if lines.is_empty() {
        return bad_input("address_lines is required");
    }
    let body = json!({
        "address": {
            "regionCode": str_arg(&args, "region_code").unwrap_or(&state.config.provider.region_code),
            "addressLines": lines,
            "locality": str_arg(&args, "locality"),
            "administrativeArea": str_arg(&args, "administrative_area"),
            "postalCode": str_arg(&args, "postal_code"),
        },
        "enableUspsCass": bool_arg(&args, "enable_usps_cass").unwrap_or(false),
    });
    state
        .google
        .post(
            spec("gmaps_address_validation", 1),
            "https://addressvalidation.googleapis.com/v1:validateAddress",
            BTreeMap::new(),
            body,
            dry_run(&args),
        )
        .await
}

async fn roads(state: &AppState, args: Value) -> ToolEnvelope {
    let mode = str_arg(&args, "mode").unwrap_or("nearest");
    let has_path = args.get("path").and_then(Value::as_array).is_some();
    let has_place_ids = args
        .get("place_ids")
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(Value::is_string));
    if has_path && has_place_ids {
        return bad_input("path and place_ids cannot be combined");
    }
    let mut endpoint = match mode {
        "snap_to_roads" => "https://roads.googleapis.com/v1/snapToRoads",
        "speed_limits" => "https://roads.googleapis.com/v1/speedLimits",
        _ => "https://roads.googleapis.com/v1/nearestRoads",
    }
    .to_string();
    let mut query = BTreeMap::new();
    if let Some(path) = args.get("path").and_then(Value::as_array) {
        query.insert(
            "path".to_string(),
            path.iter()
                .filter_map(lat_lng_query)
                .collect::<Vec<_>>()
                .join("|"),
        );
    }
    if let Some(place_ids) = args.get("place_ids").and_then(Value::as_array) {
        let place_ids = place_ids
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        if !place_ids.is_empty() {
            endpoint = format!(
                "https://roads.googleapis.com/v1/speedLimits?{}",
                place_ids
                    .iter()
                    .map(|place_id| format!("placeId={}", urlencoding::encode(place_id)))
                    .collect::<Vec<_>>()
                    .join("&")
            );
        }
    }
    if query.is_empty() && !endpoint.contains("placeId=") {
        return bad_input("path or place_ids is required");
    }
    state
        .google
        .get(spec("gmaps_roads", 1), &endpoint, query, dry_run(&args))
        .await
}

async fn route_optimization(state: &AppState, args: Value) -> ToolEnvelope {
    if state.config.provider.project_id.trim().is_empty() {
        return err(
            "CONFIG_MISSING_PROJECT_ID",
            "provider.project_id is required for route optimization",
            false,
            None,
        );
    }
    let endpoint = format!(
        "https://routeoptimization.googleapis.com/v1/projects/{}:optimizeTours",
        state.config.provider.project_id
    );
    state
        .google
        .post(
            spec("gmaps_route_optimization", 1),
            &endpoint,
            BTreeMap::new(),
            args.clone(),
            dry_run(&args),
        )
        .await
}

async fn solar(state: &AppState, args: Value) -> ToolEnvelope {
    let Some(location) = args.get("location").and_then(lat_lng_value) else {
        return bad_input("solar currently requires lat_lng location");
    };
    let mut query = BTreeMap::new();
    query.insert(
        "location.latitude".to_string(),
        location["latitude"].to_string(),
    );
    query.insert(
        "location.longitude".to_string(),
        location["longitude"].to_string(),
    );
    optional_string_query(&args, &mut query, "required_quality", "requiredQuality");
    state
        .google
        .get(
            spec("gmaps_solar", 1),
            "https://solar.googleapis.com/v1/buildingInsights:findClosest",
            query,
            dry_run(&args),
        )
        .await
}

fn spec(tool_name: &'static str, units: u32) -> crate::google::CallSpec {
    spec_for_tool(tool_name, units).expect("tool has pricing spec")
}

fn bad_input(message: impl Into<String>) -> ToolEnvelope {
    err("INVALID_INPUT", message.into(), false, None)
}

fn hard_limit(name: &str, value: u32, limit: u32) -> ToolEnvelope {
    err(
        "HARD_LIMIT_EXCEEDED",
        format!("{name}={value} exceeds hard limit {limit}"),
        false,
        None,
    )
}

fn dry_run(args: &Value) -> bool {
    bool_arg(args, "dry_run").unwrap_or(false)
}

fn bool_arg<'a>(args: &'a Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn u32_arg(args: &Value, key: &str) -> Option<u32> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn common_query(state: &AppState) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "language".to_string(),
            state.config.provider.language_code.clone(),
        ),
        (
            "region".to_string(),
            state.config.provider.region_code.clone(),
        ),
    ])
}

fn optional_string_query(
    args: &Value,
    query: &mut BTreeMap<String, String>,
    input: &str,
    output: &str,
) {
    if let Some(value) = str_arg(args, input) {
        query.insert(output.to_string(), value.to_string());
    } else if let Some(value) = args.get(input).and_then(Value::as_i64) {
        query.insert(output.to_string(), value.to_string());
    } else if let Some(value) = args.get(input).and_then(Value::as_f64) {
        query.insert(output.to_string(), value.to_string());
    }
}

fn optional_join_query(
    args: &Value,
    query: &mut BTreeMap<String, String>,
    input: &str,
    output: &str,
) {
    if let Some(values) = args.get(input).and_then(Value::as_array) {
        query.insert(
            output.to_string(),
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("|"),
        );
    }
}

fn location_query(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|value| {
        lat_lng_query(value)
            .or_else(|| {
                value
                    .get("address")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                value
                    .get("place_id")
                    .and_then(Value::as_str)
                    .map(|place_id| format!("place_id:{place_id}"))
            })
            .or_else(|| {
                value
                    .get("plus_code")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    })
}

fn location_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    lat_lng_query(value)
                        .or_else(|| value.as_str().map(str::to_string))
                        .or_else(|| {
                            value
                                .get("address")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .or_else(|| {
                            value
                                .get("place_id")
                                .and_then(Value::as_str)
                                .map(|place_id| format!("place_id:{place_id}"))
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn lat_lng_query(value: &Value) -> Option<String> {
    let lat = value
        .get("lat")
        .or_else(|| value.get("latitude"))?
        .as_f64()?;
    let lng = value
        .get("lng")
        .or_else(|| value.get("longitude"))?
        .as_f64()?;
    Some(format!("{lat},{lng}"))
}

fn lat_lng_value(value: &Value) -> Option<Value> {
    let lat = value
        .get("lat")
        .or_else(|| value.get("latitude"))?
        .as_f64()?;
    let lng = value
        .get("lng")
        .or_else(|| value.get("longitude"))?
        .as_f64()?;
    Some(json!({ "latitude": lat, "longitude": lng }))
}

fn waypoint_value(value: &Value) -> Option<Value> {
    if let Some(lat_lng) = lat_lng_value(value) {
        Some(json!({ "location": { "latLng": lat_lng } }))
    } else if let Some(address) = value.get("address").and_then(Value::as_str) {
        Some(json!({ "address": address }))
    } else {
        value
            .get("place_id")
            .and_then(Value::as_str)
            .map(|place_id| json!({ "placeId": place_id }))
    }
}
