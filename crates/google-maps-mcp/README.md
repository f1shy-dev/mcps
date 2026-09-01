# Google Maps MCP

`google-maps-mcp` is an independent Streamable HTTP MCP server for Google Maps Platform APIs. It exposes geocoding, places, autocomplete, timezone, routes, route matrix, static maps, Street View, elevation, weather, air quality, pollen, address validation, roads, route optimization, and solar tools.

Uncached live calls reserve estimated billable spend in a local SQLite monthly budget ledger before the Google request is made. The estimate applies the configured Google Maps monthly free unit cap per SKU, so early calls inside the free tier record units but `$0` estimated cost. Cache hits do not call Google or require remaining budget. Static Maps and Street View image tools only build unsigned URLs; they report estimated cost but do not record spend because the server did not fetch Google.

The default bind is `[::]:8000`, which accepts Railway's private IPv6 traffic as well as public IPv4 traffic. If `PORT` is set, the server binds to `[::]:$PORT` so the Docker image works on platforms such as Railway without a config file. Set `GOOGLE_MAPS_MCP_BEARER_TOKEN` to require MCP requests to include `Authorization: Bearer <token>`; without it, the endpoint is unauthenticated and should be protected by the deployment platform.

Most tools use `GOOGLE_MAPS_API_KEY`. Route Optimization requires an OAuth access token with the `cloud-platform` scope because Google checks the caller's `routeoptimization.locations.use` IAM permission on the target project; it reads the token from `GOOGLE_MAPS_OAUTH_TOKEN` and the project from `provider.project_id`.

## Tools

- `gmaps_usage_status`
- `gmaps_estimate_call`
- `gmaps_geocode`
- `gmaps_reverse_geocode`
- `gmaps_find_place`
- `gmaps_place_details`
- `gmaps_autocomplete`
- `gmaps_timezone`
- `gmaps_route`
- `gmaps_route_matrix`
- `gmaps_static_map`
- `gmaps_streetview`
- `gmaps_elevation`
- `gmaps_weather`
- `gmaps_air_quality`
- `gmaps_pollen`
- `gmaps_address_validation`
- `gmaps_roads`
- `gmaps_route_optimization`
- `gmaps_solar`

## Local Run

```bash
GOOGLE_MAPS_API_KEY=... cargo run -p google-maps-mcp
```

No config file is required. The built-in configuration binds to all interfaces, caps estimated spend at `$3` per month, and uses local SQLite files for the usage ledger and cache. Set `GOOGLE_MAPS_MCP_CONFIG` to override these defaults with a TOML file such as `config.example.toml`; an explicitly configured missing or invalid file is an error.

Without `GOOGLE_MAPS_API_KEY`, dry-runs and usage status still work; billable live calls fail closed. Route Optimization additionally needs `GOOGLE_MAPS_OAUTH_TOKEN` and a configured `provider.project_id`.

For authenticated network exposure:

```bash
GOOGLE_MAPS_API_KEY=... \
GOOGLE_MAPS_MCP_BEARER_TOKEN=... \
GOOGLE_MAPS_MCP_CONFIG=/path/to/config-with-non-loopback-bind.toml \
cargo run -p google-maps-mcp
```
