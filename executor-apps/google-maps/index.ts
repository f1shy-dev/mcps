import {
  array,
  boolean,
  defineApp,
  defineProvider,
  header,
  json,
  number,
  oauth2,
  object,
  ProviderError,
  query,
  type QueryContext,
  record,
  router,
  secrets,
  string,
  bearer,
  query as placeQuery,
} from "apps";

const MAPS_HOSTS = [
  "maps.googleapis.com",
  "places.googleapis.com",
  "routes.googleapis.com",
  "weather.googleapis.com",
  "airquality.googleapis.com",
  "pollen.googleapis.com",
  "addressvalidation.googleapis.com",
  "roads.googleapis.com",
  "solar.googleapis.com",
];

const googleMaps = defineProvider({
  name: "Google Maps Platform",
  hosts: MAPS_HOSTS,
  auth: {
    apiKey: secrets({
      label: "Google Maps Platform API key",
      fields: object({ apiKey: string({ minLength: 1 }) }),
      request: ({ apiKey }) => [header("x-goog-api-key", apiKey), placeQuery("key", apiKey)],
    }),
  },
  async health({ account, fetch, signal }) {
    const url = new URL("https://maps.googleapis.com/maps/api/timezone/json");
    url.searchParams.set("location", "51.5007,-0.1246");
    url.searchParams.set("timestamp", "0");
    const response = await fetch(account.url(url), { signal });
    if (response.status >= 500) throw new ProviderError({ reason: "unavailable", status: response.status });
    const body = (await response.json()) as { status?: string; error_message?: string };
    if (body.status === "OK") return;
    const message = body.error_message ?? "";
    if (/invalid/i.test(message)) throw new ProviderError({ reason: "unauthorized", status: response.status });
    if (body.status === "OVER_QUERY_LIMIT") throw new ProviderError({ reason: "rate_limited", status: response.status });
    throw new Error(
      "Google refused the Time Zone check. Enable the Time Zone API (and Places API (New), Routes, Geocoding and Weather) for this key's project, and allow those APIs in the key's API restrictions.",
    );
  },
});

const googleCloud = defineProvider({
  name: "Google Cloud (Route Optimization)",
  hosts: ["routeoptimization.googleapis.com"],
  auth: {
    oauth: oauth2({
      discover: "https://accounts.google.com",
      scopes: ["https://www.googleapis.com/auth/cloud-platform"],
      authorizationParams: { access_type: "offline", prompt: "consent" },
      request: ({ access_token }) => bearer(access_token),
    }),
  },
});

const requirements = {
  user: { accounts: { maps: googleMaps, routeOptimization: googleCloud.many() } },
};
type Context = QueryContext<typeof requirements>;

const SETTINGS = {
  languageCode: "en-GB",
  regionCode: "GB",
  units: "METRIC",
  monthlyBudgetUsd: 3,
  maxPlacesResultsDefault: 10,
  maxPlacesResultsHard: 20,
  maxWaypointsHard: 25,
  maxRouteMatrixElementsHard: 100,
  maxStaticMapWidth: 1280,
  maxStaticMapHeight: 1280,
  maxRawResponseBytes: 1_048_576,
  requestTimeoutMs: 30_000,
};

type Pricing = { sku: string; price: number; freeUnits: number; ttlSeconds: number };
const DAY = 24 * 3600;
const PRICING: Record<string, Pricing> = {
  geocode: { sku: "Geocoding", price: 0.005, freeUnits: 10_000, ttlSeconds: 30 * DAY },
  reverseGeocode: { sku: "Geocoding", price: 0.005, freeUnits: 10_000, ttlSeconds: 30 * DAY },
  findPlace: { sku: "Places", price: 0.017, freeUnits: 5_000, ttlSeconds: DAY },
  placeDetails: { sku: "Places", price: 0.017, freeUnits: 5_000, ttlSeconds: DAY },
  autocomplete: { sku: "Autocomplete", price: 0.00283, freeUnits: 10_000, ttlSeconds: DAY },
  timezone: { sku: "Time Zone", price: 0.005, freeUnits: 10_000, ttlSeconds: 30 * DAY },
  route: { sku: "Routes", price: 0.005, freeUnits: 10_000, ttlSeconds: 5 * 60 },
  routeMatrix: { sku: "Route Matrix", price: 0.005, freeUnits: 10_000, ttlSeconds: 5 * 60 },
  staticMap: { sku: "Static Maps", price: 0.002, freeUnits: 10_000, ttlSeconds: 7 * DAY },
  streetView: { sku: "Static Street View", price: 0.007, freeUnits: 10_000, ttlSeconds: 7 * DAY },
  elevation: { sku: "Elevation", price: 0.005, freeUnits: 5_000, ttlSeconds: 30 * DAY },
  weather: { sku: "Weather", price: 0.00015, freeUnits: 10_000, ttlSeconds: 15 * 60 },
  airQuality: { sku: "Air Quality", price: 0.005, freeUnits: 10_000, ttlSeconds: 15 * 60 },
  pollen: { sku: "Pollen", price: 0.01, freeUnits: 5_000, ttlSeconds: 60 * 60 },
  addressValidation: { sku: "Address Validation", price: 0.017, freeUnits: 5_000, ttlSeconds: DAY },
  roads: { sku: "Roads", price: 0.01, freeUnits: 5_000, ttlSeconds: DAY },
  routeOptimization: { sku: "Route Optimization", price: 0.01, freeUnits: 5_000, ttlSeconds: 5 * 60 },
  solar: { sku: "Solar", price: 0.01, freeUnits: 10_000, ttlSeconds: DAY },
};

const LEGACY_NAMES: Record<string, string> = {
  gmaps_geocode: "geocode",
  gmaps_reverse_geocode: "reverseGeocode",
  gmaps_find_place: "findPlace",
  gmaps_place_details: "placeDetails",
  gmaps_autocomplete: "autocomplete",
  gmaps_timezone: "timezone",
  gmaps_route: "route",
  gmaps_route_matrix: "routeMatrix",
  gmaps_static_map: "staticMap",
  gmaps_streetview: "streetView",
  gmaps_elevation: "elevation",
  gmaps_weather: "weather",
  gmaps_air_quality: "airQuality",
  gmaps_pollen: "pollen",
  gmaps_address_validation: "addressValidation",
  gmaps_roads: "roads",
  gmaps_route_optimization: "routeOptimization",
  gmaps_solar: "solar",
};

const FIELD_MASKS: Array<[string, string]> = [
  [
    "places:searchText",
    "places.id,places.displayName,places.formattedAddress,places.location,places.types,places.rating,places.userRatingCount,places.googleMapsUri",
  ],
  ["places:autocomplete", "suggestions.placePrediction.placeId,suggestions.placePrediction.text,suggestions.placePrediction.types"],
  [
    "/v1/places/",
    "id,displayName,formattedAddress,location,types,rating,userRatingCount,googleMapsUri,websiteUri,nationalPhoneNumber,internationalPhoneNumber,regularOpeningHours,businessStatus,priceLevel",
  ],
  ["directions/v2:computeRoutes", "routes.duration,routes.distanceMeters,routes.polyline.encodedPolyline,routes.description,routes.legs"],
];

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type Usage = {
  sku: string;
  units: number;
  billable_units: number;
  free_units_per_month: number;
  free_units_remaining_before_call: number;
  unit_price_usd: number;
  estimated_cost_usd: number;
  used_usd_this_month: number;
  monthly_budget_usd: number;
  remaining_budget_usd: number;
  allowed: boolean;
  denial_reason: string | null;
  dry_run: boolean;
};
type CacheInfo = { hit: boolean; ttl_seconds?: number; cache_key?: string };
type Envelope = { ok: true; data: Json; usage?: Usage; cache?: CacheInfo };
type Spec = Pricing & { tool: string; units: number };
type Query = Array<[string, string]>;

const LedgerSchema = object({ skus: record(object({ units: number(), estimated_cost_usd: number() })) });
type Ledger = { skus: Record<string, { units: number; estimated_cost_usd: number }> };
const CachedSchema = object({ value: json(), expires_at: number() });

class ToolFailure extends Error {
  constructor(code: string, message: string, retryable = false, usage?: Usage) {
    super(JSON.stringify({ ok: false, error: { code, message, retryable }, ...(usage ? { usage } : {}) }));
  }
}

const spec = (tool: string, units = 1): Spec => ({ ...PRICING[tool]!, tool, units });
const currentMonth = () => new Date().toISOString().slice(0, 7);
const ledgerKey = () => ["ledger", currentMonth()];
const accountCache = (ctx: Context) => ctx.cache.forAccount(ctx.user.accounts.maps);

const readLedger = async (ctx: Context): Promise<Ledger> =>
  ((await accountCache(ctx).read(ledgerKey(), LedgerSchema)) as Ledger | undefined) ?? { skus: {} };

const billableUnits = (usedUnits: number, units: number, freeUnits: number) =>
  Math.max(0, usedUnits + units - freeUnits) - Math.max(0, usedUnits - freeUnits);

const estimate = (ledger: Ledger, s: Spec, dryRun: boolean): Usage => {
  const used = Object.values(ledger.skus).reduce((sum, entry) => sum + entry.estimated_cost_usd, 0);
  const usedUnits = ledger.skus[s.sku]?.units ?? 0;
  const billable = billableUnits(usedUnits, s.units, s.freeUnits);
  const cost = s.price * billable;
  const allowed = used + cost <= SETTINGS.monthlyBudgetUsd;
  return {
    sku: s.sku,
    units: s.units,
    billable_units: billable,
    free_units_per_month: s.freeUnits,
    free_units_remaining_before_call: Math.max(0, s.freeUnits - usedUnits),
    unit_price_usd: s.price,
    estimated_cost_usd: cost,
    used_usd_this_month: used,
    monthly_budget_usd: SETTINGS.monthlyBudgetUsd,
    remaining_budget_usd: Math.max(0, SETTINGS.monthlyBudgetUsd - used),
    allowed,
    denial_reason: allowed ? null : "monthly_budget_exceeded",
    dry_run: dryRun,
  };
};

const reserve = async (ctx: Context, s: Spec): Promise<Usage> => {
  const ledger = await readLedger(ctx);
  const usage = estimate(ledger, s, false);
  if (!usage.allowed) return usage;
  const entry = ledger.skus[s.sku] ?? { units: 0, estimated_cost_usd: 0 };
  ledger.skus[s.sku] = { units: entry.units + s.units, estimated_cost_usd: entry.estimated_cost_usd + usage.estimated_cost_usd };
  await accountCache(ctx).write([{ key: ledgerKey(), value: ledger }], "7 days");
  return usage;
};

const buildUrl = (endpoint: string, params: Query) => {
  const url = new URL(endpoint);
  for (const [key, value] of params) url.searchParams.append(key, value);
  return url.toString();
};

const cacheKeyFor = async (s: Spec, endpoint: string, params: Query, body: Json) => {
  const text = JSON.stringify([s.tool, s.sku, s.units, endpoint, params, body]);
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
};

const isGoogleError = (value: Json) => {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  if ("error" in value) return true;
  const status = value.status;
  return typeof status === "string" && !["OK", "ZERO_RESULTS", "NOT_FOUND"].includes(status);
};

const send = async (ctx: Context, method: string, endpoint: string, params: Query, body: Json | undefined, auth: "maps" | "cloud") => {
  const headers: Record<string, string> = { accept: "application/json" };
  let url = buildUrl(endpoint, params);
  if (auth === "cloud") {
    const [account] = ctx.user.accounts.routeOptimization;
    if (!account)
      throw new ToolFailure(
        "CONFIG_MISSING_OAUTH_TOKEN",
        "Route Optimization needs a Google Cloud account with the cloud-platform scope; link one to this app's routeOptimization requirement",
      );
    Object.assign(headers, account.headers());
  } else {
    const maps = ctx.user.accounts.maps;
    const headerKey = endpoint.startsWith("https://places.googleapis.com/") || endpoint.startsWith("https://routes.googleapis.com/");
    if (headerKey) Object.assign(headers, { "x-goog-api-key": maps.headers()["x-goog-api-key"]! });
    else url = maps.url(url);
  }
  const fieldMask = FIELD_MASKS.find(([fragment]) => endpoint.includes(fragment))?.[1];
  if (fieldMask) headers["x-goog-fieldmask"] = fieldMask;
  if (body !== undefined) headers["content-type"] = "application/json";
  const signal =
    typeof AbortSignal.any === "function" ? AbortSignal.any([ctx.signal, AbortSignal.timeout(SETTINGS.requestTimeoutMs)]) : ctx.signal;
  let response: Response;
  try {
    response = await ctx.fetch(url, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal });
  } catch (error) {
    throw new ToolFailure("REQUEST_ERROR", error instanceof Error ? error.message : String(error), true);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength > SETTINGS.maxRawResponseBytes)
    throw new ToolFailure("REQUEST_ERROR", "google response exceeded max_raw_response_bytes (" + SETTINGS.maxRawResponseBytes + ")", true);
  const text = new TextDecoder().decode(bytes);
  let value: Json;
  try {
    value = JSON.parse(text) as Json;
  } catch {
    value = { text };
  }
  if (!response.ok || isGoogleError(value)) throw new ToolFailure("GOOGLE_API_ERROR", JSON.stringify(value));
  return value;
};

const call = async (
  ctx: Context,
  s: Spec,
  method: "GET" | "POST",
  endpoint: string,
  params: Query,
  body: Json | undefined,
  dryRun: boolean,
  auth: "maps" | "cloud" = "maps",
): Promise<Envelope> => {
  const cacheKey = await cacheKeyFor(s, endpoint, params, body ?? null);
  if (dryRun)
    return {
      ok: true,
      data: { dry_run: true, endpoint, method },
      usage: estimate(await readLedger(ctx), s, true),
      cache: { hit: false, cache_key: cacheKey },
    };
  const store = accountCache(ctx);
  const cached = (await store.read(["response", cacheKey], CachedSchema)) as { value: Json; expires_at: number } | undefined;
  const now = Math.floor(Date.now() / 1000);
  if (cached && cached.expires_at > now)
    return {
      ok: true,
      data: cached.value,
      usage: estimate(await readLedger(ctx), { ...s, units: 0 }, false),
      cache: { hit: true, ttl_seconds: cached.expires_at - now, cache_key: cacheKey },
    };
  const usage = await reserve(ctx, s);
  if (!usage.allowed) throw new ToolFailure("MONTHLY_BUDGET_EXCEEDED", "projected call would exceed local monthly budget", false, usage);
  const value = await send(ctx, method, endpoint, params, body, auth);
  await store
    .write([{ key: ["response", cacheKey], value: { value, expires_at: now + s.ttlSeconds } }], `${s.ttlSeconds} seconds` as const)
    .catch(() => undefined);
  return { ok: true, data: value, usage, cache: { hit: false, ttl_seconds: s.ttlSeconds, cache_key: cacheKey } };
};

const imageUrl = async (ctx: Context, s: Spec, endpoint: string, params: Query, dryRun: boolean): Promise<Envelope> => ({
  ok: true,
  data: { url: buildUrl(endpoint, params), dry_run: dryRun, billable_when_fetched: true, usage_recorded: false },
  usage: estimate(await readLedger(ctx), s, dryRun),
  cache: { hit: false, cache_key: await cacheKeyFor(s, endpoint, params, null) },
});

const badInput = (message: string) => new ToolFailure("INVALID_INPUT", message);
const hardLimit = (name: string, value: number, limit: number) =>
  new ToolFailure("HARD_LIMIT_EXCEEDED", name + "=" + value + " exceeds hard limit " + limit);

const LatLng = object({ lat: number(), lng: number() });
type LatLngValue = { lat: number; lng: number };
const Size = object({ width: number(), height: number() });
const LOCATION_HELP = "{lat,lng}, {address}, {place_id}, {plus_code} or a plain string";
const WAYPOINT_HELP = "{lat,lng}, {address} or {place_id}";

const latLngQuery = (value: LatLngValue) => value.lat + "," + value.lng;
const latLngValue = (value: LatLngValue) => ({ latitude: value.lat, longitude: value.lng });

const asObject = (value: Json): Record<string, Json> | undefined =>
  value && typeof value === "object" && !Array.isArray(value) ? value : undefined;

const asLatLng = (value: Record<string, Json>): LatLngValue | undefined => {
  const lat = value.lat ?? value.latitude;
  const lng = value.lng ?? value.longitude;
  return typeof lat === "number" && typeof lng === "number" ? { lat, lng } : undefined;
};

const locationQuery = (value: Json): string => {
  if (typeof value === "string") return value;
  const item = asObject(value);
  if (item) {
    const point = asLatLng(item);
    if (point) return latLngQuery(point);
    if (typeof item.address === "string") return item.address;
    if (typeof item.place_id === "string") return "place_id:" + item.place_id;
    if (typeof item.plus_code === "string") return item.plus_code;
  }
  throw badInput("location must be " + LOCATION_HELP);
};

const waypointValue = (value: Json): Json => {
  const item = asObject(value);
  if (item) {
    const point = asLatLng(item);
    if (point) return { location: { latLng: latLngValue(point) } };
    if (typeof item.address === "string") return { address: item.address };
    if (typeof item.place_id === "string") return { placeId: item.place_id };
  }
  throw badInput("waypoint must be " + WAYPOINT_HELP);
};

const commonQuery = (): Query => [
  ["language", SETTINGS.languageCode],
  ["region", SETTINGS.regionCode],
];

const setParam = (params: Query, key: string, value: string | undefined) => {
  if (value === undefined) return params;
  return [...params.filter(([name]) => name !== key), [key, value] as [string, string]];
};

const dryRun = { dry_run: boolean().default(false) };

const usageStatus = query(
  { description: "Report the Google Maps usage ledger and remaining monthly budget without calling Google.", input: object({}) },
  async (ctx: Context) => {
    const ledger = await readLedger(ctx);
    const spent = Object.values(ledger.skus).reduce((sum, entry) => sum + entry.estimated_cost_usd, 0);
    return {
      ok: true,
      data: {
        month: currentMonth(),
        monthly_budget_usd: SETTINGS.monthlyBudgetUsd,
        estimated_spend_usd: spent,
        remaining_budget_usd: Math.max(0, SETTINGS.monthlyBudgetUsd - spent),
        ledger_ok: true,
        skus: Object.entries(ledger.skus)
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([sku, entry]) => ({ sku, units: entry.units, estimated_cost_usd: entry.estimated_cost_usd })),
      },
    };
  },
);

const estimateCall = query(
  {
    description:
      "Estimate cost and budget allowance for a planned Google Maps tool call without calling Google. tool_name is a tool of this app (such as geocode) or its old gmaps_* name.",
    input: object({ tool_name: string(), planned_units: number().default(1) }),
  },
  async (ctx: Context, input) => {
    const tool = LEGACY_NAMES[input.tool_name] ?? input.tool_name;
    if (!PRICING[tool]) throw new ToolFailure("UNKNOWN_TOOL_OR_SKU", "no pricing estimate configured for " + input.tool_name);
    const usage = estimate(await readLedger(ctx), spec(tool, Math.max(1, Math.floor(input.planned_units))), true);
    return { ok: true, data: { notes: [], estimate: usage }, usage };
  },
);

const geocode = query(
  {
    description: "Convert an address into coordinates and Google place metadata.",
    input: object({ address: string(), ...dryRun, region_code: string().optional(), language_code: string().optional() }),
  },
  async (ctx: Context, input) => {
    let params = setParam(commonQuery(), "address", input.address);
    params = setParam(params, "region", input.region_code);
    params = setParam(params, "language", input.language_code);
    return call(ctx, spec("geocode"), "GET", "https://maps.googleapis.com/maps/api/geocode/json", params, undefined, input.dry_run);
  },
);

const reverseGeocode = query(
  {
    description: "Convert latitude/longitude into addresses and place metadata.",
    input: object({
      location: LatLng,
      ...dryRun,
      result_type: array(string()).default([]),
      location_type: array(string()).default([]),
    }),
  },
  async (ctx: Context, input) => {
    let params = setParam(commonQuery(), "latlng", latLngQuery(input.location));
    if (input.result_type.length) params = setParam(params, "result_type", input.result_type.join("|"));
    if (input.location_type.length) params = setParam(params, "location_type", input.location_type.join("|"));
    return call(ctx, spec("reverseGeocode"), "GET", "https://maps.googleapis.com/maps/api/geocode/json", params, undefined, input.dry_run);
  },
);

const findPlace = query(
  {
    description: "Search for places by text using Places API (New) text search, optionally biased near a point.",
    input: object({
      query: string(),
      ...dryRun,
      max_results: number().optional(),
      near: LatLng.optional(),
      radius_meters: number().optional(),
    }),
  },
  async (ctx: Context, input) => {
    const max = input.max_results ?? SETTINGS.maxPlacesResultsDefault;
    if (max > SETTINGS.maxPlacesResultsHard) throw hardLimit("max_results", max, SETTINGS.maxPlacesResultsHard);
    const body: Record<string, Json> = {
      textQuery: input.query,
      maxResultCount: max,
      languageCode: SETTINGS.languageCode,
      regionCode: SETTINGS.regionCode,
    };
    if (input.near)
      body.locationBias = { circle: { center: latLngValue(input.near), radius: input.radius_meters ?? 5000 } };
    return call(ctx, spec("findPlace"), "POST", "https://places.googleapis.com/v1/places:searchText", [], body, input.dry_run);
  },
);

const placeDetails = query(
  {
    description: "Fetch controlled details for a Google place ID.",
    input: object({ place_id: string(), ...dryRun, language_code: string().optional(), region_code: string().optional() }),
  },
  async (ctx: Context, input) => {
    const placeId = input.place_id.replace(/^(places\/)+/, "");
    let params: Query = [];
    params = setParam(params, "languageCode", input.language_code);
    params = setParam(params, "regionCode", input.region_code);
    return call(ctx, spec("placeDetails"), "GET", "https://places.googleapis.com/v1/places/" + placeId, params, undefined, input.dry_run);
  },
);

const autocomplete = query(
  {
    description: "Return place/address autocomplete predictions, optionally using a session token.",
    input: object({ input: string(), ...dryRun, session_token: string().optional(), location_bias: LatLng.optional() }),
  },
  async (ctx: Context, input) => {
    const body: Record<string, Json> = { input: input.input, languageCode: SETTINGS.languageCode, regionCode: SETTINGS.regionCode };
    if (input.session_token !== undefined) body.sessionToken = input.session_token;
    if (input.location_bias) body.locationBias = { circle: { center: latLngValue(input.location_bias), radius: 5000 } };
    return call(ctx, spec("autocomplete"), "POST", "https://places.googleapis.com/v1/places:autocomplete", [], body, input.dry_run);
  },
);

const timezone = query(
  {
    description: "Get timezone information for a location (" + LOCATION_HELP + "). timestamp is Unix seconds and defaults to now.",
    input: object({ location: json(), ...dryRun, timestamp: number().optional() }),
  },
  async (ctx: Context, input) => {
    let params = setParam(commonQuery(), "location", locationQuery(input.location as Json));
    params = setParam(params, "timestamp", String(Math.trunc(input.timestamp ?? Date.now() / 1000)));
    return call(ctx, spec("timezone"), "GET", "https://maps.googleapis.com/maps/api/timezone/json", params, undefined, input.dry_run);
  },
);

const route = query(
  {
    description:
      "Compute routes between locations using Google Routes API. origin, destination and waypoints are " +
      WAYPOINT_HELP +
      ". travel_mode defaults to DRIVE and routing_preference to TRAFFIC_AWARE.",
    input: object({
      origin: json(),
      destination: json(),
      ...dryRun,
      waypoints: array(json()).default([]),
      travel_mode: string().optional(),
      routing_preference: string().optional(),
      compute_alternative_routes: boolean().default(false),
      units: string().optional(),
      avoid_tolls: boolean().default(false),
      avoid_highways: boolean().default(false),
      avoid_ferries: boolean().default(false),
    }),
  },
  async (ctx: Context, input) => {
    if (input.waypoints.length > SETTINGS.maxWaypointsHard) throw hardLimit("waypoints", input.waypoints.length, SETTINGS.maxWaypointsHard);
    const body: Record<string, Json> = {
      origin: waypointValue(input.origin as Json),
      destination: waypointValue(input.destination as Json),
      travelMode: input.travel_mode ?? "DRIVE",
      routingPreference: input.routing_preference ?? "TRAFFIC_AWARE",
      computeAlternativeRoutes: input.compute_alternative_routes,
      languageCode: SETTINGS.languageCode,
      units: input.units ?? SETTINGS.units,
    };
    if (input.waypoints.length) body.intermediates = input.waypoints.map((point) => waypointValue(point as Json));
    body.routeModifiers = { avoidTolls: input.avoid_tolls, avoidHighways: input.avoid_highways, avoidFerries: input.avoid_ferries };
    return call(ctx, spec("route"), "POST", "https://routes.googleapis.com/directions/v2:computeRoutes", [], body, input.dry_run);
  },
);

const routeMatrix = query(
  {
    description:
      "Compare travel times/distances between many origins and destinations (each " + LOCATION_HELP + "). travel_mode defaults to DRIVING.",
    input: object({ origins: array(json()), destinations: array(json()), ...dryRun, travel_mode: string().optional() }),
  },
  async (ctx: Context, input) => {
    if (!input.origins.length || !input.destinations.length) throw badInput("origins and destinations are required");
    const elements = input.origins.length * input.destinations.length;
    if (elements > SETTINGS.maxRouteMatrixElementsHard) throw hardLimit("route_matrix_elements", elements, SETTINGS.maxRouteMatrixElementsHard);
    let params = setParam(commonQuery(), "origins", input.origins.map((item) => locationQuery(item as Json)).join("|"));
    params = setParam(params, "destinations", input.destinations.map((item) => locationQuery(item as Json)).join("|"));
    params = setParam(params, "mode", (input.travel_mode ?? "DRIVING").toLowerCase());
    return call(ctx, spec("routeMatrix", elements), "GET", "https://maps.googleapis.com/maps/api/distancematrix/json", params, undefined, input.dry_run);
  },
);

const staticMap = query(
  {
    description:
      "Build a Google Static Maps URL (without the API key) for a location. center is " + LOCATION_HELP + ". Size defaults to 640x640.",
    input: object({ ...dryRun, size: Size.optional(), center: json().optional(), zoom: number().optional(), map_type: string().optional() }),
  },
  async (ctx: Context, input) => {
    const width = input.size?.width ?? 640;
    const height = input.size?.height ?? 640;
    if (width > SETTINGS.maxStaticMapWidth) throw hardLimit("width", width, SETTINGS.maxStaticMapWidth);
    if (height > SETTINGS.maxStaticMapHeight) throw hardLimit("height", height, SETTINGS.maxStaticMapHeight);
    let params: Query = [["size", width + "x" + height]];
    if (input.center !== undefined) params = setParam(params, "center", locationQuery(input.center as Json));
    if (input.zoom !== undefined) params = setParam(params, "zoom", String(input.zoom));
    params = setParam(params, "maptype", input.map_type);
    return imageUrl(ctx, spec("staticMap"), "https://maps.googleapis.com/maps/api/staticmap", params, input.dry_run);
  },
);

const streetView = query(
  {
    description:
      "Build a Street View image URL (without the API key), or its metadata URL with metadata_only. location is " + LOCATION_HELP + ".",
    input: object({
      location: json(),
      ...dryRun,
      size: Size.optional(),
      heading: string().optional(),
      pitch: string().optional(),
      fov: string().optional(),
      metadata_only: boolean().default(false),
    }),
  },
  async (ctx: Context, input) => {
    let params: Query = [
      ["location", locationQuery(input.location as Json)],
      ["size", (input.size?.width ?? 640) + "x" + (input.size?.height ?? 640)],
    ];
    params = setParam(params, "heading", input.heading);
    params = setParam(params, "pitch", input.pitch);
    params = setParam(params, "fov", input.fov);
    const endpoint = input.metadata_only
      ? "https://maps.googleapis.com/maps/api/streetview/metadata"
      : "https://maps.googleapis.com/maps/api/streetview";
    return imageUrl(ctx, spec("streetView"), endpoint, params, input.dry_run);
  },
);

const elevation = query(
  {
    description: "Get elevation for points (locations) or a sampled path (path plus samples).",
    input: object({ ...dryRun, locations: array(LatLng).default([]), path: array(LatLng).default([]), samples: number().optional() }),
  },
  async (ctx: Context, input) => {
    let params: Query;
    if (input.locations.length) params = [["locations", input.locations.map(latLngQuery).join("|")]];
    else if (input.path.length) {
      params = [["path", input.path.map(latLngQuery).join("|")]];
      if (input.samples !== undefined) params = setParam(params, "samples", String(input.samples));
    } else throw badInput("locations or path is required");
    return call(ctx, spec("elevation"), "GET", "https://maps.googleapis.com/maps/api/elevation/json", params, undefined, input.dry_run);
  },
);

const weather = query(
  {
    description: "Get current weather (mode current, the default) or the hourly forecast (mode forecast) for a location.",
    input: object({ location: LatLng, ...dryRun, mode: string().optional() }),
  },
  async (ctx: Context, input) => {
    const endpoint =
      input.mode === "forecast"
        ? "https://weather.googleapis.com/v1/forecast/hours:lookup"
        : "https://weather.googleapis.com/v1/currentConditions:lookup";
    const params: Query = [
      ["location.latitude", String(input.location.lat)],
      ["location.longitude", String(input.location.lng)],
    ];
    return call(ctx, spec("weather"), "GET", endpoint, params, undefined, input.dry_run);
  },
);

const airQuality = query(
  {
    description: "Get air quality conditions for a location.",
    input: object({ location: LatLng, ...dryRun, include_pollutants: boolean().default(false) }),
  },
  async (ctx: Context, input) => {
    const body = {
      location: latLngValue(input.location),
      extraComputations: input.include_pollutants ? ["POLLUTANT_CONCENTRATION"] : [],
    };
    return call(ctx, spec("airQuality"), "POST", "https://airquality.googleapis.com/v1/currentConditions:lookup", [], body, input.dry_run);
  },
);

const pollen = query(
  {
    description: "Get pollen forecast data for a location.",
    input: object({ location: LatLng, ...dryRun, days: number().default(1) }),
  },
  async (ctx: Context, input) => {
    const params: Query = [
      ["location.latitude", String(input.location.lat)],
      ["location.longitude", String(input.location.lng)],
      ["days", String(input.days)],
    ];
    return call(ctx, spec("pollen"), "GET", "https://pollen.googleapis.com/v1/forecast:lookup", params, undefined, input.dry_run);
  },
);

const addressValidation = query(
  {
    description: "Validate and normalize a postal address. region_code defaults to GB.",
    input: object({
      address_lines: array(string()),
      ...dryRun,
      region_code: string().optional(),
      locality: string().optional(),
      administrative_area: string().optional(),
      postal_code: string().optional(),
      enable_usps_cass: boolean().default(false),
    }),
  },
  async (ctx: Context, input) => {
    if (!input.address_lines.length) throw badInput("address_lines is required");
    const body = {
      address: {
        regionCode: input.region_code ?? SETTINGS.regionCode,
        addressLines: [...input.address_lines],
        locality: input.locality ?? null,
        administrativeArea: input.administrative_area ?? null,
        postalCode: input.postal_code ?? null,
      },
      enableUspsCass: input.enable_usps_cass,
    };
    return call(ctx, spec("addressValidation"), "POST", "https://addressvalidation.googleapis.com/v1:validateAddress", [], body, input.dry_run);
  },
);

const roads = query(
  {
    description:
      "Use Roads API for nearest roads (mode nearest, the default), snap_to_roads, or speed_limits. Pass path points, or place_ids for speed limits.",
    input: object({ ...dryRun, mode: string().optional(), path: array(LatLng).default([]), place_ids: array(string()).default([]) }),
  },
  async (ctx: Context, input) => {
    const mode = input.mode ?? "nearest";
    if (input.path.length && input.place_ids.length) throw badInput("path and place_ids cannot be combined");
    let endpoint =
      mode === "snap_to_roads"
        ? "https://roads.googleapis.com/v1/snapToRoads"
        : mode === "speed_limits"
          ? "https://roads.googleapis.com/v1/speedLimits"
          : "https://roads.googleapis.com/v1/nearestRoads";
    let params: Query = [];
    if (input.path.length) params = [[mode === "nearest" ? "points" : "path", input.path.map(latLngQuery).join("|")]];
    if (input.place_ids.length) {
      endpoint = "https://roads.googleapis.com/v1/speedLimits";
      params = input.place_ids.map((placeId) => ["placeId", placeId] as [string, string]);
    }
    if (!params.length) throw badInput("path or place_ids is required");
    return call(ctx, spec("roads"), "GET", endpoint, params, undefined, input.dry_run);
  },
);

const routeOptimization = query(
  {
    description:
      "Run a Google Route Optimization optimizeTours request for small routing problems. request is the optimizeTours body (model, searchMode, ...). Needs a Google Cloud OAuth account linked to this app's routeOptimization requirement and the project_id it runs in.",
    input: object({ project_id: string({ minLength: 1 }), request: record(json()), ...dryRun }),
  },
  async (ctx: Context, input) => {
    const endpoint = "https://routeoptimization.googleapis.com/v1/projects/" + encodeURIComponent(input.project_id) + ":optimizeTours";
    return call(ctx, spec("routeOptimization"), "POST", endpoint, [], input.request as Json, input.dry_run, "cloud");
  },
);

const solar = query(
  {
    description: "Query Google Solar building insights near a location.",
    input: object({ location: LatLng, ...dryRun, required_quality: string().optional() }),
  },
  async (ctx: Context, input) => {
    let params: Query = [
      ["location.latitude", String(input.location.lat)],
      ["location.longitude", String(input.location.lng)],
    ];
    params = setParam(params, "requiredQuality", input.required_quality);
    return call(ctx, spec("solar"), "GET", "https://solar.googleapis.com/v1/buildingInsights:findClosest", params, undefined, input.dry_run);
  },
);

export default defineApp(requirements, {
  tools: router(
    {
      usageStatus,
      estimateCall,
      geocode,
      reverseGeocode,
      findPlace,
      placeDetails,
      autocomplete,
      timezone,
      route,
      routeMatrix,
      staticMap,
      streetView,
      elevation,
      weather,
      airQuality,
      pollen,
      addressValidation,
      roads,
      routeOptimization,
      solar,
    },
    {
      title: "Google Maps",
      description: "Google Maps Platform: geocoding, places, routing, imagery URLs, weather, air quality, pollen, roads and solar.",
      instructions: [
        "Every billable call is checked against a monthly budget of $3 estimated spend per linked API key before Google is called; usageStatus shows the ledger and estimateCall prices a planned call.",
        "Pass dry_run: true to any tool to see the request and its estimated cost without calling Google.",
        "Identical calls are served from a per-key cache (5 minutes for routes up to 30 days for geocoding) and cost nothing.",
        "staticMap and streetView only build URLs without the API key; they are not fetched and record no spend.",
        "Failures throw a JSON error with code, message and retryable, plus usage when the budget refused the call.",
      ].join("\n"),
    },
  ),
});
