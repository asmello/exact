use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub github_id: i64,
    pub github_login: String,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

pub async fn connect(url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(8)
        .connect(url)
        .await
        .with_context(|| format!("connecting to postgres at {url}"))
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("running migrations")
}

pub async fn upsert_github_user(
    pool: &PgPool,
    github_id: i64,
    github_login: &str,
    avatar_url: Option<&str>,
    promote_to_admin: bool,
) -> Result<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (github_id, github_login, avatar_url, is_admin)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (github_id) DO UPDATE
          SET github_login = EXCLUDED.github_login,
              avatar_url   = EXCLUDED.avatar_url,
              is_admin     = users.is_admin OR EXCLUDED.is_admin
        RETURNING id, github_id, github_login, avatar_url, is_admin, created_at
        "#,
    )
    .bind(github_id)
    .bind(github_login)
    .bind(avatar_url)
    .bind(promote_to_admin)
    .fetch_one(pool)
    .await
    .context("upserting github user")?;
    Ok(user)
}

pub async fn get_user(pool: &PgPool, id: i64) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, github_id, github_login, avatar_url, is_admin, created_at
        FROM users WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("fetching user")?;
    Ok(user)
}
