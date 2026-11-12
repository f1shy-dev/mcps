# Google Maps (Executor app)

An [Executor](https://executor.sh) app that exposes Google Maps Platform APIs as agent tools. It is a port of [`crates/google-maps-mcp`](../../crates/google-maps-mcp) with the same tools, inputs, defaults and monthly budget guard.

It is deployed on jikkou as **Google Maps** (slug `google-maps`), so agents call the tools as `tools["google-maps"].<tool>(...)`.

## Connect an API key

The app has one required account, `maps`, scoped to each **user**: every caller links their own Google Maps Platform API key, and the budget ledger and response cache belong to that key. Connect it from the app's Accounts page or through a link from `appAccounts.userConnect`. Never paste the key into chat or source.

1. In the [Google Cloud console](https://console.cloud.google.com/google/maps-apis/credentials), choose the project to bill and create an API key under **Keys & Credentials**.
2. Enable the APIs the tools use in [the API library](https://console.cloud.google.com/apis/library): **Places API (New)**, **Routes API**, **Geocoding API**, **Time Zone API** and **Weather API**. For the other tools, also enable Distance Matrix, Elevation, Air Quality, Pollen, Address Validation, Roads, Solar, Maps Static and Street View Static.
3. If the key has API restrictions, allow the same APIs.

The connect form checks the key with one Time Zone request before saving it, and says which API to enable if Google refuses it.

`routeOptimization` also needs a Google Cloud OAuth account with the `cloud-platform` scope, because Google checks the caller's IAM permission for Route Optimization and does not accept API keys. That requirement (`routeOptimization`) takes zero or more accounts, so the other tools work without it. Connecting it asks for a Google OAuth client.

## Tools

| Old MCP tool | App tool |
| --- | --- |
| `gmaps_usage_status` | `usageStatus` |
| `gmaps_estimate_call` | `estimateCall` |
| `gmaps_geocode` | `geocode` |
| `gmaps_reverse_geocode` | `reverseGeocode` |
| `gmaps_find_place` | `findPlace` |
| `gmaps_place_details` | `placeDetails` |
| `gmaps_autocomplete` | `autocomplete` |
| `gmaps_timezone` | `timezone` |
| `gmaps_route` | `route` |
| `gmaps_route_matrix` | `routeMatrix` |
| `gmaps_static_map` | `staticMap` |
| `gmaps_streetview` | `streetView` |
| `gmaps_elevation` | `elevation` |
| `gmaps_weather` | `weather` |
| `gmaps_air_quality` | `airQuality` |
| `gmaps_pollen` | `pollen` |
| `gmaps_address_validation` | `addressValidation` |
| `gmaps_roads` | `roads` |
| `gmaps_route_optimization` | `routeOptimization` |
| `gmaps_solar` | `solar` |

Every tool is a read-only query. Input field names are the MCP's (`dry_run`, `place_id`, `max_results` and so on). A successful call returns the MCP's envelope: `{ ok, data, usage, cache }`. A failed call throws an error whose message is the MCP's error envelope as JSON: `{ ok: false, error: { code, message, retryable }, usage? }`.

## Budget and cache

Each live call reserves its estimated cost in a monthly ledger before Google is called, and is refused with `MONTHLY_BUDGET_EXCEEDED` when the month's estimated spend would pass **$3**. Estimates use the same per-SKU prices and monthly free units as the MCP, so calls inside the free tier record units at $0. `dry_run: true` returns the request and its estimate without calling Google. `staticMap` and `streetView` only build URLs without the key and record no spend.

Responses are cached per key with the MCP's lifetimes (5 minutes for routes up to 30 days for geocoding). A cache hit does not call Google and costs nothing.

Differences from the MCP:

- The ledger and cache live in Executor's app cache, scoped to the linked key, not in SQLite. Executor queries cannot write the app database, and every tool here is a query. The cache keeps an entry at most 7 days after its last write, so a month's ledger resets if the key makes no live call for 7 days, and it starts empty when the key is reconnected or replaced. Concurrent calls can both pass a nearly spent budget. Treat the budget as a guard, and set a Google Cloud budget alert for a hard limit.
- Settings are constants in `index.ts` instead of a TOML file: `en-GB`, region `GB`, `METRIC` units, a $3 budget, and the MCP's default limits. Every module is enabled.
- `routeOptimization` takes `project_id` and the `optimizeTours` body as `request` in its input, because the MCP read the project from its config file and the body from top-level fields.
- `LatLng` inputs take `lat` and `lng`. Location and waypoint inputs (`timezone`, `route`, `routeMatrix`, `staticMap`, `streetView`) also accept `latitude`/`longitude`, as the MCP did.

## Deploy

The app pins `apps` to the version jikkou runs (`0.0.1-beta.122`). Type-check before deploying:

```sh
npm install --no-package-lock
npx -y -p typescript@7 tsc --ignoreConfig --noEmit --strict --skipLibCheck \
  --module nodenext --moduleResolution nodenext --target es2022 index.ts
```

Deploy with `executor apps deploy --app app_58469526-7cf0-4076-baa1-8d7bb121f70d --files .`, or through the Executor MCP with `appManagement.commit` and `appManagement.deploy`. Do not change the provider's `name`, `label`, fields, `hosts` or `request` without a reason: each is part of the provider's identity, and changing it means every user connects their key again.
