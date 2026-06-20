use std::{env, fs, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub provider: ProviderConfig,
    #[serde(default)]
    pub budget: BudgetConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub limits: LimitConfig,
    #[serde(default)]
    pub modules: ModuleConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: SocketAddr,
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_instructions")]
    pub instructions: String,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProviderConfig {
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default = "default_language_code")]
    pub language_code: String,
    #[serde(default = "default_region_code")]
    pub region_code: String,
    #[serde(default = "default_units")]
    pub units: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BudgetConfig {
    #[serde(default = "default_monthly_budget_usd")]
    pub monthly_budget_usd: f64,
    #[serde(default = "default_ledger_path")]
    pub ledger_path: PathBuf,
    #[serde(default = "default_true")]
    pub fail_closed: bool,
    #[serde(default = "default_true")]
    pub refuse_unknown_pricing: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cache_path")]
    pub path: PathBuf,
    #[serde(default = "default_cache_max_bytes")]
    pub max_bytes: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LimitConfig {
    #[serde(default = "default_places_results")]
    pub max_places_results_default: u32,
    #[serde(default = "default_places_results_hard")]
    pub max_places_results_hard: u32,
    #[serde(default = "default_waypoints")]
    pub max_waypoints_default: u32,
    #[serde(default = "default_waypoints_hard")]
    pub max_waypoints_hard: u32,
    #[serde(default = "default_matrix_elements")]
    pub max_route_matrix_elements_default: u32,
    #[serde(default = "default_matrix_elements_hard")]
    pub max_route_matrix_elements_hard: u32,
    #[serde(default = "default_image_width")]
    pub max_static_map_width: u32,
    #[serde(default = "default_image_height")]
    pub max_static_map_height: u32,
    #[serde(default = "default_raw_response_bytes")]
    pub max_raw_response_bytes: usize,
    #[serde(default = "default_timeout")]
    pub request_timeout_seconds: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModuleConfig {
    #[serde(default = "default_true")]
    pub geocoding: bool,
    #[serde(default = "default_true")]
    pub places: bool,
    #[serde(default = "default_true")]
    pub autocomplete: bool,
    #[serde(default = "default_true")]
    pub timezone: bool,
    #[serde(default = "default_true")]
    pub routes: bool,
    #[serde(default = "default_true")]
    pub route_matrix: bool,
    #[serde(default = "default_true")]
    pub static_maps: bool,
    #[serde(default = "default_true")]
    pub streetview: bool,
    #[serde(default = "default_true")]
    pub elevation: bool,
    #[serde(default = "default_true")]
    pub weather: bool,
    #[serde(default = "default_true")]
    pub air_quality: bool,
    #[serde(default = "default_true")]
    pub pollen: bool,
    #[serde(default = "default_true")]
    pub address_validation: bool,
    #[serde(default = "default_true")]
    pub roads: bool,
    #[serde(default = "default_true")]
    pub route_optimization: bool,
    #[serde(default = "default_true")]
    pub solar: bool,
}

impl Config {
    pub fn load(path: PathBuf) -> Result<Self> {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut config: Self = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.expand_paths();
        config.validate()?;
        Ok(config)
    }

    pub fn api_key(&self) -> Option<String> {
        env::var(&self.provider.api_key_env)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    pub fn module_enabled(&self, module: &str) -> bool {
        match module {
            "geocoding" => self.modules.geocoding,
            "places" => self.modules.places,
            "autocomplete" => self.modules.autocomplete,
            "timezone" => self.modules.timezone,
            "routes" => self.modules.routes,
            "route_matrix" => self.modules.route_matrix,
            "static_maps" => self.modules.static_maps,
            "streetview" => self.modules.streetview,
            "elevation" => self.modules.elevation,
            "weather" => self.modules.weather,
            "air_quality" => self.modules.air_quality,
            "pollen" => self.modules.pollen,
            "address_validation" => self.modules.address_validation,
            "roads" => self.modules.roads,
            "route_optimization" => self.modules.route_optimization,
            "solar" => self.modules.solar,
            _ => false,
        }
    }

    fn expand_paths(&mut self) {
        self.budget.ledger_path = expand_home(&self.budget.ledger_path);
        self.cache.path = expand_home(&self.cache.path);
    }

    fn validate(&self) -> Result<()> {
        if !self.budget.fail_closed {
            bail!("budget.fail_closed=false is not supported");
        }
        if !self.budget.refuse_unknown_pricing {
            bail!("budget.refuse_unknown_pricing=false is not supported");
        }
        if self.budget.monthly_budget_usd < 0.0 {
            bail!("monthly_budget_usd must not be negative");
        }
        if self.cache.max_bytes < 0 {
            bail!("cache max_bytes must not be negative");
        }
        if self.limits.max_places_results_default > self.limits.max_places_results_hard {
            bail!("max_places_results_default must be <= max_places_results_hard");
        }
        if self.limits.max_waypoints_default > self.limits.max_waypoints_hard {
            bail!("max_waypoints_default must be <= max_waypoints_hard");
        }
        if self.limits.max_route_matrix_elements_default
            > self.limits.max_route_matrix_elements_hard
        {
            bail!("max_route_matrix_elements_default must be <= max_route_matrix_elements_hard");
        }
        if self.limits.max_raw_response_bytes == 0 {
            bail!("max_raw_response_bytes must be greater than 0");
        }
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            name: default_name(),
            instructions: default_instructions(),
            allowed_origins: Vec::new(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_api_key_env(),
            project_id: String::new(),
            language_code: default_language_code(),
            region_code: default_region_code(),
            units: default_units(),
        }
    }
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            monthly_budget_usd: default_monthly_budget_usd(),
            ledger_path: default_ledger_path(),
            fail_closed: default_true(),
            refuse_unknown_pricing: default_true(),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            path: default_cache_path(),
            max_bytes: default_cache_max_bytes(),
        }
    }
}

impl Default for LimitConfig {
    fn default() -> Self {
        Self {
            max_places_results_default: default_places_results(),
            max_places_results_hard: default_places_results_hard(),
            max_waypoints_default: default_waypoints(),
            max_waypoints_hard: default_waypoints_hard(),
            max_route_matrix_elements_default: default_matrix_elements(),
            max_route_matrix_elements_hard: default_matrix_elements_hard(),
            max_static_map_width: default_image_width(),
            max_static_map_height: default_image_height(),
            max_raw_response_bytes: default_raw_response_bytes(),
            request_timeout_seconds: default_timeout(),
        }
    }
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            geocoding: true,
            places: true,
            autocomplete: true,
            timezone: true,
            routes: true,
            route_matrix: true,
            static_maps: true,
            streetview: true,
            elevation: true,
            weather: true,
            air_quality: true,
            pollen: true,
            address_validation: true,
            roads: true,
            route_optimization: true,
            solar: true,
        }
    }
}

fn expand_home(path: &PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" {
        home_dir()
    } else if let Some(rest) = text.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        path.clone()
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_bind() -> SocketAddr {
    "0.0.0.0:8000".parse().expect("valid default bind")
}
fn default_name() -> String {
    "Google Maps MCP".to_string()
}
fn default_instructions() -> String {
    "Use Google Maps tools for location search, routing, imagery, environment, and related APIs. Billable calls are refused if they exceed the local monthly budget.".to_string()
}
fn default_api_key_env() -> String {
    "GOOGLE_MAPS_API_KEY".to_string()
}
fn default_language_code() -> String {
    "en-GB".to_string()
}
fn default_region_code() -> String {
    "GB".to_string()
}
fn default_units() -> String {
    "METRIC".to_string()
}
fn default_true() -> bool {
    true
}
fn default_monthly_budget_usd() -> f64 {
    10.0
}
fn default_ledger_path() -> PathBuf {
    PathBuf::from("~/.config/google-maps-mcp/usage.sqlite")
}
fn default_cache_path() -> PathBuf {
    PathBuf::from("~/.cache/google-maps-mcp/cache.sqlite")
}
fn default_cache_max_bytes() -> i64 {
    1_073_741_824
}
fn default_places_results() -> u32 {
    10
}
fn default_places_results_hard() -> u32 {
    20
}
fn default_waypoints() -> u32 {
    10
}
fn default_waypoints_hard() -> u32 {
    25
}
fn default_matrix_elements() -> u32 {
    25
}
fn default_matrix_elements_hard() -> u32 {
    100
}
fn default_image_width() -> u32 {
    1280
}
fn default_image_height() -> u32 {
    1280
}
fn default_raw_response_bytes() -> usize {
    1_048_576
}
fn default_timeout() -> u64 {
    30
}
