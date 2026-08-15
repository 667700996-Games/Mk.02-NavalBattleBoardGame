use std::{env, net::SocketAddr, str::FromStr, time::Duration};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    Memory,
    Postgres,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub bind_addr: SocketAddr,
    pub storage_mode: StorageMode,
    pub database_url: String,
    pub redis_url: String,
    pub allowed_origins: Vec<String>,
    pub secure_cookies: bool,
    pub session_ttl: Duration,
    pub reconnect_grace: Duration,
    pub turn_duration_seconds: u32,
    pub public_base_url: String,
    pub api_requests_per_minute: u32,
    pub http_requests_per_minute_per_ip: u32,
    pub session_creations_per_minute: u32,
    pub websocket_events_per_second: u32,
    pub websocket_send_queue_capacity: usize,
    pub max_websocket_connections: usize,
    pub max_active_rooms: usize,
    pub max_matchmaking_queue: u64,
    pub completed_room_retention: Duration,
    pub matchmaking_entry_ttl: Duration,
    pub retention_sweep_interval: Duration,
    pub trust_proxy_headers: bool,
    pub distributed_coordination_required: bool,
    pub admin_token_hash: Option<String>,
}

impl Settings {
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();
        let host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|_| "SERVER_PORT must be a valid port".to_string())?;
        let bind_addr = SocketAddr::from_str(&format!("{host}:{port}"))
            .map_err(|_| "SERVER_HOST and SERVER_PORT are invalid".to_string())?;
        let storage_mode = match env::var("STORAGE_MODE")
            .unwrap_or_else(|_| "memory".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "memory" => StorageMode::Memory,
            "postgres" => StorageMode::Postgres,
            _ => return Err("STORAGE_MODE must be memory or postgres".to_string()),
        };
        let allowed_origins = env::var("ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:5173,http://127.0.0.1:5173".to_string())
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let database_url = env_or_file("DATABASE_URL")?;
        let redis_url = env_or_file("REDIS_URL")?;
        let admin_token = env_or_file("ADMIN_TOKEN")?;
        if admin_token.as_ref().is_some_and(|token| token.len() < 32) {
            return Err("ADMIN_TOKEN must contain at least 32 characters".to_string());
        }
        let production =
            env::var("DEPLOYMENT_ENV").is_ok_and(|value| value.eq_ignore_ascii_case("production"));
        let settings = Self {
            bind_addr,
            storage_mode,
            database_url: database_url
                .clone()
                .unwrap_or_else(|| "postgres://mk01:mk01@localhost:5432/mk01".to_string()),
            redis_url: redis_url
                .clone()
                .unwrap_or_else(|| "redis://localhost:6379/".to_string()),
            allowed_origins,
            secure_cookies: env::var("SECURE_COOKIES")
                .map(|value| value == "true")
                .unwrap_or(false),
            session_ttl: Duration::from_secs(env_u64("SESSION_TTL_SECONDS", 60 * 60 * 24 * 30)),
            reconnect_grace: Duration::from_secs(env_u64("RECONNECT_GRACE_SECONDS", 90)),
            turn_duration_seconds: env_u64("TURN_DURATION_SECONDS", 60).min(u64::from(u32::MAX))
                as u32,
            public_base_url: env::var("PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:5173".to_string()),
            api_requests_per_minute: env_u64("API_REQUESTS_PER_MINUTE", 240)
                .min(u64::from(u32::MAX)) as u32,
            http_requests_per_minute_per_ip: env_u64("HTTP_REQUESTS_PER_MINUTE_PER_IP", 600)
                .min(u64::from(u32::MAX)) as u32,
            session_creations_per_minute: env_u64("SESSION_CREATIONS_PER_MINUTE", 20)
                .min(u64::from(u32::MAX)) as u32,
            websocket_events_per_second: env_u64("WEBSOCKET_EVENTS_PER_SECOND", 60)
                .min(u64::from(u32::MAX)) as u32,
            websocket_send_queue_capacity: env_usize("WEBSOCKET_SEND_QUEUE_CAPACITY", 256).max(8),
            max_websocket_connections: env_usize("MAX_WEBSOCKET_CONNECTIONS", 10_000).max(1),
            max_active_rooms: env_usize("MAX_ACTIVE_ROOMS", 25_000).max(1),
            max_matchmaking_queue: env_u64("MAX_MATCHMAKING_QUEUE", 10_000).max(1),
            completed_room_retention: Duration::from_secs(env_u64(
                "COMPLETED_ROOM_RETENTION_SECONDS",
                60 * 60 * 24 * 90,
            )),
            matchmaking_entry_ttl: Duration::from_secs(env_u64(
                "MATCHMAKING_ENTRY_TTL_SECONDS",
                60 * 10,
            )),
            retention_sweep_interval: Duration::from_secs(
                env_u64("RETENTION_SWEEP_INTERVAL_SECONDS", 60 * 60).max(60),
            ),
            trust_proxy_headers: env_bool("TRUST_PROXY_HEADERS", false),
            distributed_coordination_required: env_bool("DISTRIBUTED_COORDINATION_REQUIRED", false),
            admin_token_hash: admin_token.as_deref().map(hash_secret),
        };
        if production {
            if storage_mode != StorageMode::Postgres {
                return Err("production requires STORAGE_MODE=postgres".to_string());
            }
            if database_url.is_none() || redis_url.is_none() {
                return Err(
                    "production requires DATABASE_URL[_FILE] and REDIS_URL[_FILE] from the deployment secret store"
                        .to_string(),
                );
            }
            if admin_token.is_none() {
                return Err(
                    "production requires ADMIN_TOKEN[_FILE] from the deployment secret store"
                        .to_string(),
                );
            }
            if !settings.secure_cookies {
                return Err("production requires SECURE_COOKIES=true".to_string());
            }
            if !settings.distributed_coordination_required {
                return Err(
                    "production requires DISTRIBUTED_COORDINATION_REQUIRED=true".to_string()
                );
            }
            if !settings.public_base_url.starts_with("https://")
                || settings
                    .allowed_origins
                    .iter()
                    .any(|origin| !origin.starts_with("https://"))
            {
                return Err("production public URL and allowed origins must use HTTPS".to_string());
            }
        }
        Ok(settings)
    }
}

fn hash_secret(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn env_or_file(key: &str) -> Result<Option<String>, String> {
    let file_key = format!("{key}_FILE");
    if let Ok(path) = env::var(&file_key) {
        let value = std::fs::read_to_string(&path)
            .map_err(|_| format!("{file_key} could not be read"))?
            .trim()
            .to_string();
        if value.is_empty() {
            return Err(format!("{file_key} contains an empty secret"));
        }
        return Ok(Some(value));
    }
    Ok(env::var(key).ok().filter(|value| !value.trim().is_empty()))
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

impl Default for Settings {
    fn default() -> Self {
        Self::from_env().expect("default server settings must be valid")
    }
}
