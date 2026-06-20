# Google Maps MCP

`google-maps-mcp` is an independent Streamable HTTP MCP server for Google Maps Platform APIs. It exposes geocoding, places, autocomplete, timezone, routes, route matrix, static maps, Street View, elevation, weather, air quality, pollen, address validation, roads, route optimization, and solar tools.

Uncached live calls reserve estimated billable spend in a local SQLite monthly budget ledger before the Google request is made. The estimate applies the configured Google Maps monthly free unit cap per SKU, so early calls inside the free tier record units but `$0` estimated cost. Cache hits do not call Google or require remaining budget. Static Maps and Street View image tools only build unsigned URLs; they report estimated cost but do not record spend because the server did not fetch Google.

The default bind is `127.0.0.1:8000`. If `server.bind` is changed to a non-loopback address, `GOOGLE_MAPS_MCP_BEARER_TOKEN` must be set and MCP requests must include `Authorization: Bearer <token>`.

Most tools use `GOOGLE_MAPS_API_KEY`. Route Optimization requires OAuth and reads `GOOGLE_MAPS_OAUTH_TOKEN`.

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
GOOGLE_MAPS_API_KEY=... \
GOOGLE_MAPS_MCP_CONFIG=crates/google-maps-mcp/config.example.toml \
cargo run -p google-maps-mcp
```

Without `GOOGLE_MAPS_API_KEY`, dry-runs and usage status still work; billable live calls fail closed.

For network exposure:

```bash
GOOGLE_MAPS_API_KEY=... \
GOOGLE_MAPS_MCP_BEARER_TOKEN=... \
GOOGLE_MAPS_MCP_CONFIG=/path/to/config-with-non-loopback-bind.toml \
cargo run -p google-maps-mcp
```
