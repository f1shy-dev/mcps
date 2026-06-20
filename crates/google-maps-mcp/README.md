# Google Maps MCP

`google-maps-mcp` is an independent Streamable HTTP MCP server for Google Maps Platform APIs. It exposes geocoding, places, autocomplete, timezone, routes, route matrix, static maps, Street View, elevation, weather, air quality, pollen, address validation, roads, route optimization, and solar tools.

All billable calls pass through a local SQLite monthly budget gate before the Google request is made. Cache hits do not call Google.

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
