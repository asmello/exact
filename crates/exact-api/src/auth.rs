// GitHub OAuth flow + signed-cookie sessions.
//
// We use axum-extra's SignedCookieJar for stateless sessions: a single cookie
// `exact_session` carries the user id, HMAC-signed by `Key` derived from
// SESSION_SECRET. No server-side session store — restarts don't log users out.

use anyhow::{Context, Result};
use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum_extra::extract::SignedCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use serde::Deserialize;
use tracing::warn;

use crate::AppState;
use crate::db;

const SESSION_COOKIE: &str = "exact_session";
const OAUTH_STATE_COOKIE: &str = "exact_oauth_state";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/github", get(start))
        .route("/auth/github/callback", get(callback))
        .route("/auth/logout", post(logout))
}

pub fn build_oauth_client(
    backend_public_url: &url::Url,
    client_id: &str,
    client_secret: &str,
) -> Result<BasicClient> {
    let redirect = backend_public_url
        .join("/auth/github/callback")
        .context("joining redirect URL")?;
    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
        AuthUrl::new("https://github.com/login/oauth/authorize".into())
            .context("github authorize URL")?,
        Some(
            TokenUrl::new("https://github.com/login/oauth/access_token".into())
                .context("github token URL")?,
        ),
    )
    .set_redirect_uri(RedirectUrl::new(redirect.to_string()).context("github redirect URL")?);
    Ok(client)
}

async fn start(State(state): State<AppState>, jar: SignedCookieJar) -> Response {
    let (url, csrf_token) = state
        .oauth
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("read:user".to_string()))
        .url();

    let cookie = Cookie::build((OAUTH_STATE_COOKIE, csrf_token.secret().clone()))
        .path("/")
        .same_site(SameSite::Lax)
        .http_only(true)
        .secure(!cfg!(debug_assertions))
        .max_age(time::Duration::minutes(10))
        .build();

    (jar.add(cookie), Redirect::to(url.as_str())).into_response()
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let stored = match jar.get(OAUTH_STATE_COOKIE) {
        Some(c) => c.value().to_string(),
        None => return (StatusCode::BAD_REQUEST, "missing oauth state cookie").into_response(),
    };
    let jar = jar.remove(Cookie::build(OAUTH_STATE_COOKIE).path("/").build());

    if stored != q.state {
        warn!("oauth state mismatch");
        return (StatusCode::BAD_REQUEST, "oauth state mismatch").into_response();
    }

    let token = match state
        .oauth
        .exchange_code(AuthorizationCode::new(q.code))
        .request_async(async_http_client)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            warn!(error=%e, "code exchange failed");
            return (StatusCode::BAD_REQUEST, "code exchange failed").into_response();
        }
    };

    let access_token = token.access_token().secret();
    let gh_user = match fetch_github_user(access_token).await {
        Ok(u) => u,
        Err(e) => {
            warn!(error=%e, "github /user fetch failed");
            return (StatusCode::BAD_GATEWAY, "github /user fetch failed").into_response();
        }
    };

    let promote = state
        .config
        .bootstrap_admin_github_login
        .as_deref()
        .map(|l| l.eq_ignore_ascii_case(&gh_user.login))
        .unwrap_or(false);

    let user = match db::upsert_github_user(
        &state.db,
        gh_user.id,
        &gh_user.login,
        gh_user.avatar_url.as_deref(),
        promote,
    )
    .await
    {
        Ok(u) => u,
        Err(e) => {
            warn!(error=%e, "upsert github user failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "user upsert failed").into_response();
        }
    };

    let session_cookie = Cookie::build((SESSION_COOKIE, user.id.to_string()))
        .path("/")
        .same_site(SameSite::Lax)
        .http_only(true)
        .secure(!cfg!(debug_assertions))
        .max_age(time::Duration::days(30))
        .build();

    (jar.add(session_cookie), Redirect::to("/")).into_response()
}

async fn logout(jar: SignedCookieJar) -> impl IntoResponse {
    let jar = jar.remove(Cookie::build(SESSION_COOKIE).path("/").build());
    (jar, StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct GithubUser {
    id: i64,
    login: String,
    avatar_url: Option<String>,
}

async fn fetch_github_user(access_token: &str) -> Result<GithubUser> {
    let client = reqwest::Client::new();
    let user: GithubUser = client
        .get("https://api.github.com/user")
        .header("User-Agent", "exact/0.1 (exact.run)")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(access_token)
        .send()
        .await
        .context("GET https://api.github.com/user")?
        .error_for_status()
        .context("github /user non-2xx")?
        .json()
        .await
        .context("decoding github /user json")?;
    Ok(user)
}

pub async fn current_user(jar: &SignedCookieJar, state: &AppState) -> Option<db::User> {
    let cookie = jar.get(SESSION_COOKIE)?;
    let id: i64 = cookie.value().parse().ok()?;
    db::get_user(&state.db, id).await.ok().flatten()
}

pub async fn me(State(state): State<AppState>, jar: SignedCookieJar) -> Response {
    match current_user(&jar, &state).await {
        Some(u) => Json(u).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}
