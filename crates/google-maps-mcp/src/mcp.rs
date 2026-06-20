use std::{collections::BTreeMap, sync::Arc};

use axum::{Json, extract::State, http::HeaderMap, response::Response};
use mcp_shared::{
    AccessPolicy, JsonRpcError, JsonRpcRequest, JsonRpcResponse, PROTOCOL_VERSION, ToolDefinition,
    handle_streamable_http_with_access, invalid_params, method_not_found, parse_tool_input,
    response_from_result, tool_definition, tool_result,
};
use schemars::JsonSchema;
use serde::Deserialize;
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

pub async fn handle_mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let allowed_origins = state.config.server.allowed_origins.clone();
    let bearer_token = state.config.bearer_token();
    let access = AccessPolicy {
        allowed_origins: &allowed_origins,
        bearer_token: bearer_token.as_deref(),
    };
    handle_streamable_http_with_access(headers, message, &access, |request| {
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

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EstimateInput {
    tool_name: String,
    #[serde(default = "one")]
    planned_units: u32,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LatLng {
    #[serde(alias = "latitude")]
    lat: f64,
    #[serde(alias = "longitude")]
    lng: f64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AddressLocation {
    address: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PlaceLocation {
    place_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PlusCodeLocation {
    plus_code: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
enum LocationInput {
    LatLng(LatLng),
    Address(AddressLocation),
    PlaceId(PlaceLocation),
    PlusCode(PlusCodeLocation),
    Text(String),
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
enum WaypointInput {
    LatLng(LatLng),
    Address(AddressLocation),
    PlaceId(PlaceLocation),
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SizeInput {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GeocodeInput {
    address: String,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    region_code: Option<String>,
    #[serde(default)]
    language_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReverseGeocodeInput {
    location: LatLng,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    result_type: Vec<String>,
    #[serde(default)]
    location_type: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FindPlaceInput {
    query: String,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    max_results: Option<u32>,
    #[serde(default)]
    near: Option<LatLng>,
    #[serde(default)]
    radius_meters: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PlaceDetailsInput {
    place_id: String,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    language_code: Option<String>,
    #[serde(default)]
    region_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AutocompleteInput {
    input: String,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    session_token: Option<String>,
    #[serde(default)]
    location_bias: Option<LatLng>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TimezoneInput {
    location: LocationInput,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    timestamp: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RouteInput {
    origin: WaypointInput,
    destination: WaypointInput,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    waypoints: Vec<WaypointInput>,
    #[serde(default)]
    travel_mode: Option<String>,
    #[serde(default)]
    routing_preference: Option<String>,
    #[serde(default)]
    compute_alternative_routes: bool,
    #[serde(default)]
    units: Option<String>,
    #[serde(default)]
    avoid_tolls: bool,
    #[serde(default)]
    avoid_highways: bool,
    #[serde(default)]
    avoid_ferries: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RouteMatrixInput {
    origins: Vec<LocationInput>,
    destinations: Vec<LocationInput>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    travel_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StaticMapInput {
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    size: Option<SizeInput>,
    #[serde(default)]
    center: Option<LocationInput>,
    #[serde(default)]
    zoom: Option<u32>,
    #[serde(default)]
    map_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StreetViewInput {
    location: LocationInput,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    size: Option<SizeInput>,
    #[serde(default)]
    heading: Option<String>,
    #[serde(default)]
    pitch: Option<String>,
    #[serde(default)]
    fov: Option<String>,
    #[serde(default)]
    metadata_only: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ElevationInput {
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    locations: Vec<LatLng>,
    #[serde(default)]
    path: Vec<LatLng>,
    #[serde(default)]
    samples: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WeatherInput {
    location: LatLng,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AirQualityInput {
    location: LatLng,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    include_pollutants: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PollenInput {
    location: LatLng,
    #[serde(default)]
    dry_run: bool,
    #[serde(default = "one")]
    days: u32,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AddressValidationInput {
    address_lines: Vec<String>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    region_code: Option<String>,
    #[serde(default)]
    locality: Option<String>,
    #[serde(default)]
    administrative_area: Option<String>,
    #[serde(default)]
    postal_code: Option<String>,
    #[serde(default)]
    enable_usps_cass: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RoadsInput {
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    path: Vec<LatLng>,
    #[serde(default)]
    place_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct RouteOptimizationInput {
    #[serde(default)]
    dry_run: bool,
    #[serde(flatten)]
    body: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SolarInput {
    location: LatLng,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    required_quality: Option<String>,
}

fn tools() -> Vec<ToolDefinition> {
    vec![
        tool_definition::<EmptyInput>(
            "gmaps_usage_status",
            "Report local Google Maps usage ledger and remaining monthly budget without calling Google.",
        ),
        tool_definition::<EstimateInput>(
            "gmaps_estimate_call",
            "Estimate cost and budget allowance for a planned Google Maps tool call without calling Google.",
        ),
        tool_definition::<GeocodeInput>(
            "gmaps_geocode",
            "Convert an address into coordinates and Google place metadata.",
        ),
        tool_definition::<ReverseGeocodeInput>(
            "gmaps_reverse_geocode",
            "Convert latitude/longitude into addresses and place metadata.",
        ),
        tool_definition::<FindPlaceInput>(
            "gmaps_find_place",
            "Find places, businesses, POIs, or landmarks by text query.",
        ),
        tool_definition::<PlaceDetailsInput>(
            "gmaps_place_details",
            "Fetch controlled details for a Google place ID.",
        ),
        tool_definition::<AutocompleteInput>(
            "gmaps_autocomplete",
            "Return place/address autocomplete predictions, optionally using a session token.",
        ),
        tool_definition::<TimezoneInput>(
            "gmaps_timezone",
            "Get timezone information for a location.",
        ),
        tool_definition::<RouteInput>(
            "gmaps_route",
            "Compute routes between locations using Google Routes API.",
        ),
        tool_definition::<RouteMatrixInput>(
            "gmaps_route_matrix",
            "Compare travel times/distances between many origins and destinations.",
        ),
        tool_definition::<StaticMapInput>(
            "gmaps_static_map",
            "Build a Google Static Maps URL for a location, markers, or route.",
        ),
        tool_definition::<StreetViewInput>(
            "gmaps_streetview",
            "Build a Street View image URL or fetch Street View metadata.",
        ),
        tool_definition::<ElevationInput>(
            "gmaps_elevation",
            "Get elevation for points or sampled paths.",
        ),
        tool_definition::<WeatherInput>(
            "gmaps_weather",
            "Get current weather or forecast for a location.",
        ),
        tool_definition::<AirQualityInput>(
            "gmaps_air_quality",
            "Get air quality conditions for a location.",
        ),
        tool_definition::<PollenInput>("gmaps_pollen", "Get pollen forecast data for a location."),
        tool_definition::<AddressValidationInput>(
            "gmaps_address_validation",
            "Validate and normalize a postal address.",
        ),
        tool_definition::<RoadsInput>(
            "gmaps_roads",
            "Use Roads API for nearest roads, snap-to-roads, or speed limits.",
        ),
        tool_definition::<RouteOptimizationInput>(
            "gmaps_route_optimization",
            "Run a Google Route Optimization request for small routing problems.",
        ),
        tool_definition::<SolarInput>(
            "gmaps_solar",
            "Query Google Solar building insights near a location.",
        ),
    ]
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
        "gmaps_usage_status" => {
            let _input: EmptyInput = parse_tool_input(name, args)?;
            state.google.usage_status()
        }
        "gmaps_estimate_call" => {
            let input: EstimateInput = parse_tool_input(name, args)?;
            state
                .google
                .estimate_tool(&input.tool_name, input.planned_units)
        }
        "gmaps_geocode" => geocode(state, parse_tool_input(name, args)?).await,
        "gmaps_reverse_geocode" => reverse_geocode(state, parse_tool_input(name, args)?).await,
        "gmaps_find_place" => find_place(state, parse_tool_input(name, args)?).await,
        "gmaps_place_details" => place_details(state, parse_tool_input(name, args)?).await,
        "gmaps_autocomplete" => autocomplete(state, parse_tool_input(name, args)?).await,
        "gmaps_timezone" => timezone(state, parse_tool_input(name, args)?).await,
        "gmaps_route" => route(state, parse_tool_input(name, args)?).await,
        "gmaps_route_matrix" => route_matrix(state, parse_tool_input(name, args)?).await,
        "gmaps_static_map" => static_map(state, parse_tool_input(name, args)?).await,
        "gmaps_streetview" => streetview(state, parse_tool_input(name, args)?).await,
        "gmaps_elevation" => elevation(state, parse_tool_input(name, args)?).await,
        "gmaps_weather" => weather(state, parse_tool_input(name, args)?).await,
        "gmaps_air_quality" => air_quality(state, parse_tool_input(name, args)?).await,
        "gmaps_pollen" => pollen(state, parse_tool_input(name, args)?).await,
        "gmaps_address_validation" => {
            address_validation(state, parse_tool_input(name, args)?).await
        }
        "gmaps_roads" => roads(state, parse_tool_input(name, args)?).await,
        "gmaps_route_optimization" => {
            route_optimization(state, parse_tool_input(name, args)?).await
        }
        "gmaps_solar" => solar(state, parse_tool_input(name, args)?).await,
        other => return Err(invalid_params(format!("unknown tool: {other}"))),
    };
    Ok(tool_result(&envelope, !envelope.ok))
}

async fn geocode(state: &AppState, input: GeocodeInput) -> ToolEnvelope {
    let mut query = common_query(state);
    query.insert("address".to_string(), input.address);
    optional_query_value(&mut query, "region", input.region_code);
    optional_query_value(&mut query, "language", input.language_code);
    state
        .google
        .get(
            spec("gmaps_geocode", 1),
            "https://maps.googleapis.com/maps/api/geocode/json",
            query,
            input.dry_run,
        )
        .await
}

async fn reverse_geocode(state: &AppState, input: ReverseGeocodeInput) -> ToolEnvelope {
    let mut query = common_query(state);
    query.insert("latlng".to_string(), lat_lng_query(&input.location));
    optional_join_query(&mut query, "result_type", &input.result_type);
    optional_join_query(&mut query, "location_type", &input.location_type);
    state
        .google
        .get(
            spec("gmaps_reverse_geocode", 1),
            "https://maps.googleapis.com/maps/api/geocode/json",
            query,
            input.dry_run,
        )
        .await
}

async fn find_place(state: &AppState, input: FindPlaceInput) -> ToolEnvelope {
    let max = input
        .max_results
        .unwrap_or(state.config.limits.max_places_results_default);
    if max > state.config.limits.max_places_results_hard {
        return hard_limit(
            "max_results",
            max,
            state.config.limits.max_places_results_hard,
        );
    }
    let mut body = json!({
        "textQuery": input.query,
        "maxResultCount": max,
        "languageCode": state.config.provider.language_code,
        "regionCode": state.config.provider.region_code,
    });
    if let Some(near) = input.near {
        body["locationBias"] = json!({
            "circle": {
                "center": lat_lng_value(&near),
                "radius": input.radius_meters.unwrap_or(5000)
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
            input.dry_run,
        )
        .await
}

async fn place_details(state: &AppState, input: PlaceDetailsInput) -> ToolEnvelope {
    let place_id = input.place_id.trim_start_matches("places/");
    let endpoint = format!("https://places.googleapis.com/v1/places/{place_id}");
    let mut query = BTreeMap::new();
    optional_query_value(&mut query, "languageCode", input.language_code);
    optional_query_value(&mut query, "regionCode", input.region_code);
    state
        .google
        .get(
            spec("gmaps_place_details", 1),
            &endpoint,
            query,
            input.dry_run,
        )
        .await
}

async fn autocomplete(state: &AppState, input: AutocompleteInput) -> ToolEnvelope {
    let mut body = json!({
        "input": input.input,
        "languageCode": state.config.provider.language_code,
        "regionCode": state.config.provider.region_code,
    });
    if let Some(token) = input.session_token {
        body["sessionToken"] = json!(token);
    }
    if let Some(bias) = input.location_bias {
        body["locationBias"] =
            json!({ "circle": { "center": lat_lng_value(&bias), "radius": 5000 } });
    }
    state
        .google
        .post(
            spec("gmaps_autocomplete", 1),
            "https://places.googleapis.com/v1/places:autocomplete",
            BTreeMap::new(),
            body,
            input.dry_run,
        )
        .await
}

async fn timezone(state: &AppState, input: TimezoneInput) -> ToolEnvelope {
    let mut query = common_query(state);
    query.insert("location".to_string(), location_query(&input.location));
    query.insert(
        "timestamp".to_string(),
        input
            .timestamp
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
            .to_string(),
    );
    state
        .google
        .get(
            spec("gmaps_timezone", 1),
            "https://maps.googleapis.com/maps/api/timezone/json",
            query,
            input.dry_run,
        )
        .await
}

async fn route(state: &AppState, input: RouteInput) -> ToolEnvelope {
    if input.waypoints.len() as u32 > state.config.limits.max_waypoints_hard {
        return hard_limit(
            "waypoints",
            input.waypoints.len() as u32,
            state.config.limits.max_waypoints_hard,
        );
    }
    let waypoints = input
        .waypoints
        .iter()
        .map(waypoint_value)
        .collect::<Vec<_>>();
    let mut body = json!({
        "origin": waypoint_value(&input.origin),
        "destination": waypoint_value(&input.destination),
        "travelMode": input.travel_mode.as_deref().unwrap_or("DRIVE"),
        "routingPreference": input.routing_preference.as_deref().unwrap_or("TRAFFIC_AWARE"),
        "computeAlternativeRoutes": input.compute_alternative_routes,
        "languageCode": state.config.provider.language_code,
        "units": input.units.as_deref().unwrap_or(&state.config.provider.units),
    });
    if !waypoints.is_empty() {
        body["intermediates"] = json!(waypoints);
    }
    body["routeModifiers"] = json!({
        "avoidTolls": input.avoid_tolls,
        "avoidHighways": input.avoid_highways,
        "avoidFerries": input.avoid_ferries,
    });
    state
        .google
        .post(
            spec("gmaps_route", 1),
            "https://routes.googleapis.com/directions/v2:computeRoutes",
            BTreeMap::new(),
            body,
            input.dry_run,
        )
        .await
}

async fn route_matrix(state: &AppState, input: RouteMatrixInput) -> ToolEnvelope {
    if input.origins.is_empty() || input.destinations.is_empty() {
        return bad_input("origins and destinations are required");
    }
    let elements = (input.origins.len() * input.destinations.len()) as u32;
    if elements > state.config.limits.max_route_matrix_elements_hard {
        return hard_limit(
            "route_matrix_elements",
            elements,
            state.config.limits.max_route_matrix_elements_hard,
        );
    }
    let mut query = common_query(state);
    query.insert(
        "origins".to_string(),
        location_list(&input.origins).join("|"),
    );
    query.insert(
        "destinations".to_string(),
        location_list(&input.destinations).join("|"),
    );
    query.insert(
        "mode".to_string(),
        input
            .travel_mode
            .as_deref()
            .unwrap_or("DRIVING")
            .to_lowercase(),
    );
    state
        .google
        .get(
            spec("gmaps_route_matrix", elements),
            "https://maps.googleapis.com/maps/api/distancematrix/json",
            query,
            input.dry_run,
        )
        .await
}

async fn static_map(state: &AppState, input: StaticMapInput) -> ToolEnvelope {
    let width = input.size.as_ref().map(|size| size.width).unwrap_or(640);
    let height = input.size.as_ref().map(|size| size.height).unwrap_or(640);
    if width > state.config.limits.max_static_map_width {
        return hard_limit("width", width, state.config.limits.max_static_map_width);
    }
    if height > state.config.limits.max_static_map_height {
        return hard_limit("height", height, state.config.limits.max_static_map_height);
    }
    let mut query = BTreeMap::new();
    query.insert("size".to_string(), format!("{width}x{height}"));
    if let Some(center) = input.center {
        query.insert("center".to_string(), location_query(&center));
    }
    if let Some(zoom) = input.zoom {
        query.insert("zoom".to_string(), zoom.to_string());
    }
    if let Some(map_type) = input.map_type {
        query.insert("maptype".to_string(), map_type);
    }
    state
        .google
        .get_image_url(
            spec("gmaps_static_map", 1),
            "https://maps.googleapis.com/maps/api/staticmap",
            query,
            input.dry_run,
        )
        .await
}

async fn streetview(state: &AppState, input: StreetViewInput) -> ToolEnvelope {
    let width = input.size.as_ref().map(|size| size.width).unwrap_or(640);
    let height = input.size.as_ref().map(|size| size.height).unwrap_or(640);
    let mut query = BTreeMap::new();
    query.insert("location".to_string(), location_query(&input.location));
    query.insert("size".to_string(), format!("{width}x{height}"));
    optional_query_value(&mut query, "heading", input.heading);
    optional_query_value(&mut query, "pitch", input.pitch);
    optional_query_value(&mut query, "fov", input.fov);
    let endpoint = if input.metadata_only {
        "https://maps.googleapis.com/maps/api/streetview/metadata"
    } else {
        "https://maps.googleapis.com/maps/api/streetview"
    };
    state
        .google
        .get_image_url(spec("gmaps_streetview", 1), endpoint, query, input.dry_run)
        .await
}

async fn elevation(state: &AppState, input: ElevationInput) -> ToolEnvelope {
    let mut query = BTreeMap::new();
    if !input.locations.is_empty() {
        query.insert(
            "locations".to_string(),
            input
                .locations
                .iter()
                .map(lat_lng_query)
                .collect::<Vec<_>>()
                .join("|"),
        );
    } else if !input.path.is_empty() {
        query.insert(
            "path".to_string(),
            input
                .path
                .iter()
                .map(lat_lng_query)
                .collect::<Vec<_>>()
                .join("|"),
        );
        optional_query_value(
            &mut query,
            "samples",
            input.samples.map(|samples| samples.to_string()),
        );
    } else {
        return bad_input("locations or path is required");
    }
    state
        .google
        .get(
            spec("gmaps_elevation", 1),
            "https://maps.googleapis.com/maps/api/elevation/json",
            query,
            input.dry_run,
        )
        .await
}

async fn weather(state: &AppState, input: WeatherInput) -> ToolEnvelope {
    let mode = input.mode.as_deref().unwrap_or("current");
    let endpoint = if mode == "forecast" {
        "https://weather.googleapis.com/v1/forecast/hours:lookup"
    } else {
        "https://weather.googleapis.com/v1/currentConditions:lookup"
    };
    let mut query = BTreeMap::new();
    query.insert(
        "location.latitude".to_string(),
        input.location.lat.to_string(),
    );
    query.insert(
        "location.longitude".to_string(),
        input.location.lng.to_string(),
    );
    state
        .google
        .get(spec("gmaps_weather", 1), endpoint, query, input.dry_run)
        .await
}

async fn air_quality(state: &AppState, input: AirQualityInput) -> ToolEnvelope {
    let body = json!({
        "location": lat_lng_value(&input.location),
        "extraComputations": if input.include_pollutants {
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
            input.dry_run,
        )
        .await
}

async fn pollen(state: &AppState, input: PollenInput) -> ToolEnvelope {
    let mut query = BTreeMap::new();
    query.insert(
        "location.latitude".to_string(),
        input.location.lat.to_string(),
    );
    query.insert(
        "location.longitude".to_string(),
        input.location.lng.to_string(),
    );
    query.insert("days".to_string(), input.days.to_string());
    state
        .google
        .get(
            spec("gmaps_pollen", 1),
            "https://pollen.googleapis.com/v1/forecast:lookup",
            query,
            input.dry_run,
        )
        .await
}

async fn address_validation(state: &AppState, input: AddressValidationInput) -> ToolEnvelope {
    if input.address_lines.is_empty() {
        return bad_input("address_lines is required");
    }
    let body = json!({
        "address": {
            "regionCode": input.region_code.as_deref().unwrap_or(&state.config.provider.region_code),
            "addressLines": input.address_lines,
            "locality": input.locality,
            "administrativeArea": input.administrative_area,
            "postalCode": input.postal_code,
        },
        "enableUspsCass": input.enable_usps_cass,
    });
    state
        .google
        .post(
            spec("gmaps_address_validation", 1),
            "https://addressvalidation.googleapis.com/v1:validateAddress",
            BTreeMap::new(),
            body,
            input.dry_run,
        )
        .await
}

async fn roads(state: &AppState, input: RoadsInput) -> ToolEnvelope {
    let mode = input.mode.as_deref().unwrap_or("nearest");
    let has_path = !input.path.is_empty();
    let has_place_ids = !input.place_ids.is_empty();
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
    if !input.path.is_empty() {
        query.insert(
            "path".to_string(),
            input
                .path
                .iter()
                .map(lat_lng_query)
                .collect::<Vec<_>>()
                .join("|"),
        );
    }
    if !input.place_ids.is_empty() {
        endpoint = format!(
            "https://roads.googleapis.com/v1/speedLimits?{}",
            input
                .place_ids
                .iter()
                .map(|place_id| format!("placeId={}", urlencoding::encode(place_id)))
                .collect::<Vec<_>>()
                .join("&")
        );
    }
    if query.is_empty() && !endpoint.contains("placeId=") {
        return bad_input("path or place_ids is required");
    }
    state
        .google
        .get(spec("gmaps_roads", 1), &endpoint, query, input.dry_run)
        .await
}

async fn route_optimization(state: &AppState, input: RouteOptimizationInput) -> ToolEnvelope {
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
            Value::Object(input.body.into_iter().collect()),
            input.dry_run,
        )
        .await
}

async fn solar(state: &AppState, input: SolarInput) -> ToolEnvelope {
    let mut query = BTreeMap::new();
    query.insert(
        "location.latitude".to_string(),
        input.location.lat.to_string(),
    );
    query.insert(
        "location.longitude".to_string(),
        input.location.lng.to_string(),
    );
    optional_query_value(&mut query, "requiredQuality", input.required_quality);
    state
        .google
        .get(
            spec("gmaps_solar", 1),
            "https://solar.googleapis.com/v1/buildingInsights:findClosest",
            query,
            input.dry_run,
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

fn optional_query_value(query: &mut BTreeMap<String, String>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        query.insert(key.to_string(), value);
    }
}

fn optional_join_query(query: &mut BTreeMap<String, String>, key: &str, values: &[String]) {
    if !values.is_empty() {
        query.insert(key.to_string(), values.join("|"));
    }
}

fn location_query(location: &LocationInput) -> String {
    match location {
        LocationInput::LatLng(lat_lng) => lat_lng_query(lat_lng),
        LocationInput::Address(location) => location.address.clone(),
        LocationInput::PlaceId(location) => format!("place_id:{}", location.place_id),
        LocationInput::PlusCode(location) => location.plus_code.clone(),
        LocationInput::Text(value) => value.clone(),
    }
}

fn location_list(values: &[LocationInput]) -> Vec<String> {
    values.iter().map(location_query).collect()
}

fn lat_lng_query(value: &LatLng) -> String {
    format!("{},{}", value.lat, value.lng)
}

fn lat_lng_value(value: &LatLng) -> Value {
    json!({ "latitude": value.lat, "longitude": value.lng })
}

fn waypoint_value(value: &WaypointInput) -> Value {
    match value {
        WaypointInput::LatLng(lat_lng) => {
            json!({ "location": { "latLng": lat_lng_value(lat_lng) } })
        }
        WaypointInput::Address(location) => json!({ "address": location.address }),
        WaypointInput::PlaceId(location) => json!({ "placeId": location.place_id }),
    }
}

fn one() -> u32 {
    1
}
