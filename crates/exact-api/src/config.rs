use anyhow::{Context, Result, bail};
use url::Url;

pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub backend_public_url: Url,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub session_secret: Vec<u8>,
    pub bootstrap_admin_github_login: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let backend_public_url: Url = std::env::var("BACKEND_PUBLIC_URL")
            .context("BACKEND_PUBLIC_URL must be set")?
            .parse()
            .context("parsing BACKEND_PUBLIC_URL")?;
        let github_client_id =
            std::env::var("GITHUB_CLIENT_ID").context("GITHUB_CLIENT_ID must be set")?;
        let github_client_secret =
            std::env::var("GITHUB_CLIENT_SECRET").context("GITHUB_CLIENT_SECRET must be set")?;

        let secret = std::env::var("SESSION_SECRET").context("SESSION_SECRET must be set")?;
        let session_secret = secret.into_bytes();
        if session_secret.len() < 64 {
            bail!(
                "SESSION_SECRET must be >=64 bytes (HMAC key); got {}",
                session_secret.len()
            );
        }

        let bootstrap_admin_github_login = std::env::var("BOOTSTRAP_ADMIN_GITHUB_LOGIN").ok();

        Ok(Self {
            bind_addr,
            database_url,
            backend_public_url,
            github_client_id,
            github_client_secret,
            session_secret,
            bootstrap_admin_github_login,
        })
    }
}
