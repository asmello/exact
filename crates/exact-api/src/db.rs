use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub github_id: i64,
    pub github_login: String,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Problem {
    pub id: String,
    pub title: String,
    pub description_md: String,
    pub starter_code: String,
    pub io_spec: serde_json::Value,
    pub visibility: String,
    pub share_token: Option<String>,
    pub default_timeout_ms: i32,
    pub allowed_boards: Vec<String>,
    pub owner_id: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TestCase {
    pub id: i64,
    pub problem_id: String,
    pub ord: i32,
    pub name: Option<String>,
    pub input: Vec<u8>,
    pub expected_output: Option<Vec<u8>>,
    pub weight: f32,
    pub hidden: bool,
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

// ---- Problems ------------------------------------------------------------

const PROBLEM_COLS: &str = "id, title, description_md, starter_code, io_spec, visibility, \
    share_token, default_timeout_ms, allowed_boards, owner_id, created_at, updated_at";

pub struct NewProblem<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub description_md: &'a str,
    pub starter_code: &'a str,
    pub io_spec: &'a serde_json::Value,
    pub visibility: &'a str,
    pub default_timeout_ms: i32,
    pub allowed_boards: &'a [String],
    pub owner_id: i64,
}

pub async fn create_problem(pool: &PgPool, p: NewProblem<'_>) -> Result<Problem> {
    let row = sqlx::query_as::<_, Problem>(&format!(
        "INSERT INTO problems (id, title, description_md, starter_code, io_spec, visibility, \
          default_timeout_ms, allowed_boards, owner_id) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING {PROBLEM_COLS}"
    ))
    .bind(p.id)
    .bind(p.title)
    .bind(p.description_md)
    .bind(p.starter_code)
    .bind(p.io_spec)
    .bind(p.visibility)
    .bind(p.default_timeout_ms)
    .bind(p.allowed_boards)
    .bind(p.owner_id)
    .fetch_one(pool)
    .await
    .context("inserting problem")?;
    Ok(row)
}

pub struct ProblemUpdate<'a> {
    pub title: Option<&'a str>,
    pub description_md: Option<&'a str>,
    pub starter_code: Option<&'a str>,
    pub io_spec: Option<&'a serde_json::Value>,
    pub visibility: Option<&'a str>,
    pub default_timeout_ms: Option<i32>,
    pub allowed_boards: Option<&'a [String]>,
}

pub async fn update_problem(pool: &PgPool, id: &str, u: ProblemUpdate<'_>) -> Result<Problem> {
    // COALESCE-style update keeps fields the caller didn't supply.
    let row = sqlx::query_as::<_, Problem>(&format!(
        "UPDATE problems SET \
            title              = COALESCE($2, title), \
            description_md     = COALESCE($3, description_md), \
            starter_code       = COALESCE($4, starter_code), \
            io_spec            = COALESCE($5, io_spec), \
            visibility         = COALESCE($6, visibility), \
            default_timeout_ms = COALESCE($7, default_timeout_ms), \
            allowed_boards     = COALESCE($8, allowed_boards), \
            updated_at         = now() \
         WHERE id = $1 RETURNING {PROBLEM_COLS}"
    ))
    .bind(id)
    .bind(u.title)
    .bind(u.description_md)
    .bind(u.starter_code)
    .bind(u.io_spec)
    .bind(u.visibility)
    .bind(u.default_timeout_ms)
    .bind(u.allowed_boards)
    .fetch_one(pool)
    .await
    .context("updating problem")?;
    Ok(row)
}

pub async fn delete_problem(pool: &PgPool, id: &str) -> Result<u64> {
    let res = sqlx::query("DELETE FROM problems WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("deleting problem")?;
    Ok(res.rows_affected())
}

pub async fn get_problem(pool: &PgPool, id: &str) -> Result<Option<Problem>> {
    let row = sqlx::query_as::<_, Problem>(&format!(
        "SELECT {PROBLEM_COLS} FROM problems WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("fetching problem")?;
    Ok(row)
}

/// List problems visible to `viewer`. Admins see everything; otherwise
/// public problems and your own. Shared problems aren't listed — they're
/// accessed by direct URL with a share token.
pub async fn list_problems(
    pool: &PgPool,
    viewer: Option<i64>,
    is_admin: bool,
) -> Result<Vec<Problem>> {
    let rows = sqlx::query_as::<_, Problem>(&format!(
        "SELECT {PROBLEM_COLS} FROM problems \
         WHERE $2 = true \
            OR visibility = 'public' \
            OR ($1 IS NOT NULL AND owner_id = $1) \
         ORDER BY updated_at DESC"
    ))
    .bind(viewer)
    .bind(is_admin)
    .fetch_all(pool)
    .await
    .context("listing problems")?;
    Ok(rows)
}

// ---- Test cases ----------------------------------------------------------

const CASE_COLS: &str = "id, problem_id, ord, name, input, expected_output, weight, hidden";

pub struct NewTestCase<'a> {
    pub problem_id: &'a str,
    pub ord: i32,
    pub name: Option<&'a str>,
    pub input: &'a [u8],
    pub expected_output: Option<&'a [u8]>,
    pub weight: f32,
    pub hidden: bool,
}

pub async fn create_test_case(pool: &PgPool, c: NewTestCase<'_>) -> Result<TestCase> {
    let row = sqlx::query_as::<_, TestCase>(&format!(
        "INSERT INTO test_cases (problem_id, ord, name, input, expected_output, weight, hidden) \
         VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING {CASE_COLS}"
    ))
    .bind(c.problem_id)
    .bind(c.ord)
    .bind(c.name)
    .bind(c.input)
    .bind(c.expected_output)
    .bind(c.weight)
    .bind(c.hidden)
    .fetch_one(pool)
    .await
    .context("inserting test case")?;
    Ok(row)
}

pub struct TestCaseUpdate<'a> {
    pub name: Option<Option<&'a str>>,
    pub input: Option<&'a [u8]>,
    pub expected_output: Option<Option<&'a [u8]>>,
    pub weight: Option<f32>,
    pub hidden: Option<bool>,
}

pub async fn update_test_case(
    pool: &PgPool,
    problem_id: &str,
    ord: i32,
    u: TestCaseUpdate<'_>,
) -> Result<Option<TestCase>> {
    // For name/expected_output (which are themselves nullable), the wrapping
    // `Option<Option<_>>` distinguishes "leave as-is" (outer None) from
    // "explicitly set to NULL" (outer Some(None)).
    let (set_name, name_val) = match u.name {
        Some(v) => (true, v),
        None => (false, None),
    };
    let (set_expected, expected_val) = match u.expected_output {
        Some(v) => (true, v),
        None => (false, None),
    };

    let row = sqlx::query_as::<_, TestCase>(&format!(
        "UPDATE test_cases SET \
            name            = CASE WHEN $3 THEN $4 ELSE name END, \
            input           = COALESCE($5, input), \
            expected_output = CASE WHEN $6 THEN $7 ELSE expected_output END, \
            weight          = COALESCE($8, weight), \
            hidden          = COALESCE($9, hidden) \
         WHERE problem_id = $1 AND ord = $2 RETURNING {CASE_COLS}"
    ))
    .bind(problem_id)
    .bind(ord)
    .bind(set_name)
    .bind(name_val)
    .bind(u.input)
    .bind(set_expected)
    .bind(expected_val)
    .bind(u.weight)
    .bind(u.hidden)
    .fetch_optional(pool)
    .await
    .context("updating test case")?;
    Ok(row)
}

pub async fn delete_test_case(pool: &PgPool, problem_id: &str, ord: i32) -> Result<u64> {
    let res = sqlx::query("DELETE FROM test_cases WHERE problem_id = $1 AND ord = $2")
        .bind(problem_id)
        .bind(ord)
        .execute(pool)
        .await
        .context("deleting test case")?;
    Ok(res.rows_affected())
}

pub async fn list_test_cases(pool: &PgPool, problem_id: &str) -> Result<Vec<TestCase>> {
    let rows = sqlx::query_as::<_, TestCase>(&format!(
        "SELECT {CASE_COLS} FROM test_cases WHERE problem_id = $1 ORDER BY ord"
    ))
    .bind(problem_id)
    .fetch_all(pool)
    .await
    .context("listing test cases")?;
    Ok(rows)
}

// ---- Submissions ---------------------------------------------------------

/// Submission row sans `bin_blob`. The .bin is fetched separately when
/// needed (only by the runner dispatcher) to keep this struct cheap to
/// serialize over JSON.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Submission {
    pub id: Uuid,
    pub user_id: i64,
    pub problem_id: Option<String>,
    pub source_code: String,
    pub board: String,
    pub device_id: Option<String>,
    pub status: String,
    pub build_log: Option<String>,
    pub total_cycles: Option<i64>,
    pub passed: Option<i32>,
    pub total_cases: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

const SUBMISSION_COLS: &str = "id, user_id, problem_id, source_code, board, device_id, \
    status, build_log, total_cycles, passed, total_cases, created_at, finished_at";

pub async fn create_submission(
    pool: &PgPool,
    id: Uuid,
    user_id: i64,
    problem_id: Option<&str>,
    source_code: &str,
    board: &str,
) -> Result<Submission> {
    let row = sqlx::query_as::<_, Submission>(&format!(
        "INSERT INTO submissions (id, user_id, problem_id, source_code, board, status) \
         VALUES ($1,$2,$3,$4,$5,'queued') RETURNING {SUBMISSION_COLS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(problem_id)
    .bind(source_code)
    .bind(board)
    .fetch_one(pool)
    .await
    .context("inserting submission")?;
    Ok(row)
}

pub async fn get_submission(pool: &PgPool, id: Uuid) -> Result<Option<Submission>> {
    let row = sqlx::query_as::<_, Submission>(&format!(
        "SELECT {SUBMISSION_COLS} FROM submissions WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("fetching submission")?;
    Ok(row)
}

pub async fn set_submission_status(pool: &PgPool, id: Uuid, status: &str) -> Result<()> {
    sqlx::query("UPDATE submissions SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await
        .context("updating submission status")?;
    Ok(())
}

pub async fn set_submission_built(pool: &PgPool, id: Uuid, bin: &[u8]) -> Result<()> {
    sqlx::query(
        "UPDATE submissions SET status = 'done', bin_blob = $1, finished_at = now() \
         WHERE id = $2",
    )
    .bind(bin)
    .bind(id)
    .execute(pool)
    .await
    .context("marking submission built")?;
    Ok(())
}

/// Fetch the packed `.bin` for a submission. Only used by the runner
/// dispatcher; never returned to a browser.
#[allow(dead_code)] // wired up in step 6
pub async fn get_submission_bin(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>> {
    let row: Option<(Option<Vec<u8>>,)> =
        sqlx::query_as("SELECT bin_blob FROM submissions WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("fetching submission bin_blob")?;
    Ok(row.and_then(|(b,)| b))
}

pub async fn set_submission_failed(pool: &PgPool, id: Uuid, log: &str) -> Result<()> {
    sqlx::query(
        "UPDATE submissions SET status = 'failed', build_log = $1, finished_at = now() \
         WHERE id = $2",
    )
    .bind(log)
    .bind(id)
    .execute(pool)
    .await
    .context("marking submission failed")?;
    Ok(())
}
