use std::{env, net::SocketAddr, str::FromStr, time::Duration};

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
    pub public_base_url: String,
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

        Ok(Self {
            bind_addr,
            storage_mode,
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://mk01:mk01@localhost:5432/mk01".to_string()),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379/".to_string()),
            allowed_origins,
            secure_cookies: env::var("SECURE_COOKIES")
                .map(|value| value == "true")
                .unwrap_or(false),
            session_ttl: Duration::from_secs(env_u64("SESSION_TTL_SECONDS", 60 * 60 * 24 * 30)),
            reconnect_grace: Duration::from_secs(env_u64("RECONNECT_GRACE_SECONDS", 90)),
            public_base_url: env::var("PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:5173".to_string()),
        })
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

impl Default for Settings {
    fn default() -> Self {
        Self::from_env().expect("default server settings must be valid")
    }
}
