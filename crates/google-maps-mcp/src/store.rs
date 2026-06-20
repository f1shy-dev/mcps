use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::Config;

pub struct Store {
    usage: Mutex<Connection>,
    cache: Option<Mutex<Connection>>,
    cache_max_bytes: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageEstimate {
    pub sku: String,
    pub units: u32,
    pub billable_units: u32,
    pub free_units_per_month: u32,
    pub free_units_remaining_before_call: u32,
    pub unit_price_usd: f64,
    pub estimated_cost_usd: f64,
    pub used_usd_this_month: f64,
    pub monthly_budget_usd: f64,
    pub remaining_budget_usd: f64,
    pub allowed: bool,
    pub denial_reason: Option<String>,
    pub dry_run: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CacheInfo {
    pub hit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub value: Value,
    pub ttl_seconds: Option<i64>,
}

impl Store {
    pub fn open(config: &Config) -> Result<Self> {
        if let Some(parent) = config.budget.ledger_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create ledger dir {}", parent.display()))?;
            secure_path(parent)?;
        }
        let usage = Connection::open(&config.budget.ledger_path).with_context(|| {
            format!(
                "failed to open usage ledger {}",
                config.budget.ledger_path.display()
            )
        })?;
        secure_path(&config.budget.ledger_path)?;
        init_usage(&usage)?;

        let cache = if config.cache.enabled {
            if let Some(parent) = config.cache.path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
                secure_path(parent)?;
            }
            let cache = Connection::open(&config.cache.path)
                .with_context(|| format!("failed to open cache {}", config.cache.path.display()))?;
            secure_path(&config.cache.path)?;
            init_cache(&cache)?;
            Some(Mutex::new(cache))
        } else {
            None
        };

        Ok(Self {
            usage: Mutex::new(usage),
            cache,
            cache_max_bytes: config.cache.max_bytes,
        })
    }

    pub fn usage_status(&self, monthly_budget_usd: f64) -> Result<Value> {
        let month = current_month();
        let usage = self.usage_conn()?;
        let mut stmt = usage.prepare(
            "SELECT sku, units, estimated_cost_usd FROM monthly_usage WHERE month = ? ORDER BY sku",
        )?;
        let rows = stmt
            .query_map([&month], |row| {
                Ok(json!({
                    "sku": row.get::<_, String>(0)?,
                    "units": row.get::<_, i64>(1)?,
                    "estimated_cost_usd": row.get::<_, f64>(2)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let spent = self.spent_usd_locked(&usage, &month)?;
        Ok(json!({
            "month": month,
            "monthly_budget_usd": monthly_budget_usd,
            "estimated_spend_usd": spent,
            "remaining_budget_usd": (monthly_budget_usd - spent).max(0.0),
            "ledger_ok": true,
            "skus": rows,
        }))
    }

    pub fn estimate(
        &self,
        sku: &str,
        units: u32,
        unit_price_usd: f64,
        free_units_per_month: u32,
        monthly_budget_usd: f64,
        dry_run: bool,
    ) -> Result<UsageEstimate> {
        let month = current_month();
        let usage = self.usage_conn()?;
        let used = self.spent_usd_locked(&usage, &month)?;
        let used_units = sku_units(&usage, &month, sku)?;
        let billable_units = billable_units_for_call(used_units, units, free_units_per_month);
        let estimated = unit_price_usd * billable_units as f64;
        let projected = used + estimated;
        let allowed = projected <= monthly_budget_usd;
        Ok(UsageEstimate {
            sku: sku.to_string(),
            units,
            billable_units,
            free_units_per_month,
            free_units_remaining_before_call: free_units_per_month
                .saturating_sub(used_units.min(u32::MAX as u64) as u32),
            unit_price_usd,
            estimated_cost_usd: estimated,
            used_usd_this_month: used,
            monthly_budget_usd,
            remaining_budget_usd: (monthly_budget_usd - used).max(0.0),
            allowed,
            denial_reason: if allowed {
                None
            } else {
                Some("monthly_budget_exceeded".to_string())
            },
            dry_run,
        })
    }

    pub fn reserve_usage(
        &self,
        tool_name: &str,
        sku: &str,
        units: u32,
        unit_price_usd: f64,
        free_units_per_month: u32,
        monthly_budget_usd: f64,
        request_hash: &str,
    ) -> Result<UsageEstimate> {
        let month = current_month();
        let ts = Utc::now().to_rfc3339();
        let mut usage = self.usage_conn()?;
        let tx = usage.transaction()?;
        let used = spent_usd(&tx, &month)?;
        let used_units = sku_units(&tx, &month, sku)?;
        let billable_units = billable_units_for_call(used_units, units, free_units_per_month);
        let estimated = unit_price_usd * billable_units as f64;
        let projected = used + estimated;
        let allowed = projected <= monthly_budget_usd;
        let estimate = UsageEstimate {
            sku: sku.to_string(),
            units,
            billable_units,
            free_units_per_month,
            free_units_remaining_before_call: free_units_per_month
                .saturating_sub(used_units.min(u32::MAX as u64) as u32),
            unit_price_usd,
            estimated_cost_usd: estimated,
            used_usd_this_month: used,
            monthly_budget_usd,
            remaining_budget_usd: (monthly_budget_usd - used).max(0.0),
            allowed,
            denial_reason: if allowed {
                None
            } else {
                Some("monthly_budget_exceeded".to_string())
            },
            dry_run: false,
        };
        if !allowed {
            return Ok(estimate);
        }
        tx.execute(
            "INSERT INTO usage_events
             (ts, month, tool_name, sku, units, estimated_cost_usd, cache_hit, request_hash, status, error_code)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, 'reserved', NULL)",
            params![ts, month, tool_name, sku, units, estimated, request_hash],
        )?;
        tx.execute(
            "INSERT INTO monthly_usage (month, sku, units, estimated_cost_usd)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(month, sku) DO UPDATE SET
               units = units + excluded.units,
               estimated_cost_usd = estimated_cost_usd + excluded.estimated_cost_usd",
            params![month, sku, units, estimated],
        )?;
        tx.commit()?;
        Ok(estimate)
    }

    pub fn record_usage(
        &self,
        tool_name: &str,
        estimate: &UsageEstimate,
        cache_hit: bool,
        request_hash: &str,
        status: &str,
        error_code: Option<&str>,
    ) -> Result<()> {
        let month = current_month();
        let ts = Utc::now().to_rfc3339();
        let mut usage = self.usage_conn()?;
        let tx = usage.transaction()?;
        tx.execute(
            "INSERT INTO usage_events
             (ts, month, tool_name, sku, units, estimated_cost_usd, cache_hit, request_hash, status, error_code)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                ts,
                month,
                tool_name,
                estimate.sku,
                estimate.units,
                if cache_hit { 0.0 } else { estimate.estimated_cost_usd },
                if cache_hit { 1 } else { 0 },
                request_hash,
                status,
                error_code,
            ],
        )?;
        if !cache_hit && status == "ok" {
            tx.execute(
                "INSERT INTO monthly_usage (month, sku, units, estimated_cost_usd)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(month, sku) DO UPDATE SET
                   units = units + excluded.units,
                   estimated_cost_usd = estimated_cost_usd + excluded.estimated_cost_usd",
                params![
                    month,
                    estimate.sku,
                    estimate.units,
                    estimate.estimated_cost_usd
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn cache_get(&self, key: &str) -> Result<Option<CacheEntry>> {
        let Some(cache) = &self.cache else {
            return Ok(None);
        };
        let cache = cache.lock().expect("cache mutex poisoned");
        let now = Utc::now().timestamp();
        let row: Option<(String, i64)> = cache
            .query_row(
                "SELECT response_json, expires_at FROM cache_entries WHERE key = ?",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((text, expires_at)) = row else {
            return Ok(None);
        };
        if expires_at <= now {
            cache.execute("DELETE FROM cache_entries WHERE key = ?", [key])?;
            return Ok(None);
        }
        cache.execute(
            "UPDATE cache_entries SET last_accessed = ? WHERE key = ?",
            params![now, key],
        )?;
        Ok(Some(CacheEntry {
            value: serde_json::from_str(&text)?,
            ttl_seconds: Some(expires_at - now),
        }))
    }

    pub fn cache_put(&self, key: &str, ttl_seconds: i64, value: &Value) -> Result<()> {
        let Some(cache) = &self.cache else {
            return Ok(());
        };
        let text = serde_json::to_string(value)?;
        let now = Utc::now().timestamp();
        let expires_at = now + ttl_seconds.max(1);
        let cache = cache.lock().expect("cache mutex poisoned");
        cache.execute(
            "INSERT INTO cache_entries (key, response_json, size_bytes, expires_at, last_accessed)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
               response_json = excluded.response_json,
               size_bytes = excluded.size_bytes,
               expires_at = excluded.expires_at,
               last_accessed = excluded.last_accessed",
            params![key, text, text.len() as i64, expires_at, now],
        )?;
        self.evict_cache_locked(&cache)?;
        Ok(())
    }

    fn usage_conn(&self) -> Result<MutexGuard<'_, Connection>> {
        Ok(self.usage.lock().expect("usage mutex poisoned"))
    }

    fn spent_usd_locked(&self, usage: &Connection, month: &str) -> Result<f64> {
        spent_usd(usage, month)
    }

    fn evict_cache_locked(&self, cache: &Connection) -> Result<()> {
        if self.cache_max_bytes == 0 {
            cache.execute("DELETE FROM cache_entries", [])?;
            return Ok(());
        }
        loop {
            let total: i64 = cache.query_row(
                "SELECT COALESCE(SUM(size_bytes), 0) FROM cache_entries",
                [],
                |row| row.get(0),
            )?;
            if total <= self.cache_max_bytes {
                break;
            }
            let deleted = cache.execute(
                "DELETE FROM cache_entries
                 WHERE key IN (
                   SELECT key FROM cache_entries ORDER BY last_accessed ASC LIMIT 16
                 )",
                [],
            )?;
            if deleted == 0 {
                break;
            }
        }
        Ok(())
    }
}

fn init_usage(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS usage_events (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           ts TEXT NOT NULL,
           month TEXT NOT NULL,
           tool_name TEXT NOT NULL,
           sku TEXT NOT NULL,
           units INTEGER NOT NULL,
           estimated_cost_usd REAL NOT NULL,
           cache_hit INTEGER NOT NULL DEFAULT 0,
           request_hash TEXT,
           status TEXT NOT NULL,
           error_code TEXT
         );
         CREATE TABLE IF NOT EXISTS monthly_usage (
           month TEXT NOT NULL,
           sku TEXT NOT NULL,
           units INTEGER NOT NULL DEFAULT 0,
           estimated_cost_usd REAL NOT NULL DEFAULT 0,
           PRIMARY KEY (month, sku)
         );",
    )?;
    Ok(())
}

fn init_cache(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cache_entries (
           key TEXT PRIMARY KEY,
           response_json TEXT NOT NULL,
           size_bytes INTEGER NOT NULL,
           expires_at INTEGER NOT NULL,
           last_accessed INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_cache_expires_at ON cache_entries(expires_at);
         CREATE INDEX IF NOT EXISTS idx_cache_last_accessed ON cache_entries(last_accessed);",
    )?;
    Ok(())
}

pub fn current_month() -> String {
    Utc::now().format("%Y-%m").to_string()
}

fn spent_usd(conn: &Connection, month: &str) -> Result<f64> {
    Ok(conn
        .query_row(
            "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM monthly_usage WHERE month = ?",
            [month],
            |row| row.get(0),
        )
        .unwrap_or(0.0))
}

fn sku_units(conn: &Connection, month: &str, sku: &str) -> Result<u64> {
    let units = conn
        .query_row(
            "SELECT COALESCE(units, 0) FROM monthly_usage WHERE month = ? AND sku = ?",
            params![month, sku],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    Ok(units.max(0) as u64)
}

fn billable_units_for_call(used_units: u64, request_units: u32, free_units_per_month: u32) -> u32 {
    let free_units = free_units_per_month as u64;
    let before = used_units.saturating_sub(free_units);
    let after = used_units
        .saturating_add(request_units as u64)
        .saturating_sub(free_units);
    after.saturating_sub(before).min(u32::MAX as u64) as u32
}

fn secure_path(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path)?;
        let mode = if metadata.is_dir() { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::billable_units_for_call;

    #[test]
    fn free_cap_makes_early_calls_zero_cost() {
        assert_eq!(billable_units_for_call(0, 1, 10_000), 0);
        assert_eq!(billable_units_for_call(9_999, 1, 10_000), 0);
    }

    #[test]
    fn only_units_above_free_cap_are_billable() {
        assert_eq!(billable_units_for_call(9_999, 3, 10_000), 2);
        assert_eq!(billable_units_for_call(10_000, 3, 10_000), 3);
        assert_eq!(billable_units_for_call(12_000, 3, 10_000), 3);
    }
}
