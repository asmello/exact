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

/// Mark a submission as built: bin is stored, status moves to 'ready'.
/// The dispatcher then claims it ('ready' → 'running') when a runner is
/// available; on RunResult we finalize ('running' → 'done').
pub async fn set_submission_built(pool: &PgPool, id: Uuid, bin: &[u8]) -> Result<()> {
    sqlx::query("UPDATE submissions SET status = 'ready', bin_blob = $1 WHERE id = $2")
        .bind(bin)
        .bind(id)
        .execute(pool)
        .await
        .context("marking submission built")?;
    Ok(())
}

/// Fetch the packed `.bin` for a submission. Only used by the runner
/// dispatcher; never returned to a browser.
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

/// Atomic state transition: only flip 'ready' → 'running' if the row is
/// still 'ready'. Two dispatchers racing to claim the same submission
/// would both find it 'ready' but only one's UPDATE would affect a row.
pub async fn try_claim_submission(pool: &PgPool, id: Uuid, device_id: &str) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE submissions SET status = 'running', device_id = $1 \
         WHERE id = $2 AND status = 'ready'",
    )
    .bind(device_id)
    .bind(id)
    .execute(pool)
    .await
    .context("claim submission")?;
    Ok(res.rows_affected() == 1)
}

pub async fn finalize_submission(
    pool: &PgPool,
    id: Uuid,
    total_cycles: Option<i64>,
    passed: i32,
    total_cases: i32,
) -> Result<()> {
    sqlx::query(
        "UPDATE submissions SET status = 'done', total_cycles = $1, passed = $2, \
         total_cases = $3, finished_at = now() WHERE id = $4",
    )
    .bind(total_cycles)
    .bind(passed)
    .bind(total_cases)
    .bind(id)
    .execute(pool)
    .await
    .context("finalize submission")?;
    Ok(())
}

/// Submissions that finished building but have no runner assigned yet.
/// Called when a runner connects: drain anything matching its board.
pub async fn list_ready_for_board(pool: &PgPool, board: &str) -> Result<Vec<Submission>> {
    let rows = sqlx::query_as::<_, Submission>(&format!(
        "SELECT {SUBMISSION_COLS} FROM submissions \
         WHERE status = 'ready' AND board = $1 ORDER BY created_at",
    ))
    .bind(board)
    .fetch_all(pool)
    .await
    .context("listing ready submissions")?;
    Ok(rows)
}

// ---- Devices -------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Device {
    pub id: String,
    pub board: String,
    pub cclk_hz: i64,
    pub description: Option<String>,
    pub active: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub synthetic: bool,
}

pub async fn upsert_device(
    pool: &PgPool,
    id: &str,
    board: &str,
    cclk_hz: i64,
    description: Option<&str>,
    synthetic: bool,
) -> Result<Device> {
    let row = sqlx::query_as::<_, Device>(
        r#"
        INSERT INTO devices (id, board, cclk_hz, description, synthetic)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (id) DO UPDATE
          SET board = EXCLUDED.board,
              cclk_hz = EXCLUDED.cclk_hz,
              description = EXCLUDED.description,
              synthetic = EXCLUDED.synthetic
        RETURNING id, board, cclk_hz, description, active, last_seen, synthetic
        "#,
    )
    .bind(id)
    .bind(board)
    .bind(cclk_hz)
    .bind(description)
    .bind(synthetic)
    .fetch_one(pool)
    .await
    .context("upsert device")?;
    Ok(row)
}

pub async fn get_device(pool: &PgPool, id: &str) -> Result<Option<Device>> {
    let row = sqlx::query_as::<_, Device>(
        "SELECT id, board, cclk_hz, description, active, last_seen, synthetic \
         FROM devices WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("get device")?;
    Ok(row)
}

pub async fn list_devices(pool: &PgPool) -> Result<Vec<Device>> {
    let rows = sqlx::query_as::<_, Device>(
        "SELECT id, board, cclk_hz, description, active, last_seen, synthetic \
         FROM devices ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("list devices")?;
    Ok(rows)
}

pub async fn touch_device(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query("UPDATE devices SET last_seen = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("touch device")?;
    Ok(())
}

// ---- Runners (per-runner bearer tokens) ----------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Runner {
    pub id: i64,
    pub device_id: String,
    pub label: String,
    pub token_prefix: String,
    pub created_by: i64,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub async fn create_runner(
    pool: &PgPool,
    device_id: &str,
    label: &str,
    token_hash: &[u8],
    token_prefix: &str,
    created_by: i64,
) -> Result<Runner> {
    let row = sqlx::query_as::<_, Runner>(
        r#"
        INSERT INTO runners (device_id, label, token_hash, token_prefix, created_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, device_id, label, token_prefix, created_by, created_at,
                  revoked_at, last_used_at
        "#,
    )
    .bind(device_id)
    .bind(label)
    .bind(token_hash)
    .bind(token_prefix)
    .bind(created_by)
    .fetch_one(pool)
    .await
    .context("create runner")?;
    Ok(row)
}

pub async fn list_runners(pool: &PgPool) -> Result<Vec<Runner>> {
    let rows = sqlx::query_as::<_, Runner>(
        "SELECT id, device_id, label, token_prefix, created_by, created_at, \
                revoked_at, last_used_at FROM runners ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("list runners")?;
    Ok(rows)
}

pub async fn revoke_runner(pool: &PgPool, id: i64) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE runners SET revoked_at = now() \
         WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("revoke runner")?;
    Ok(res.rows_affected())
}

/// Authenticate a runner. Returns the full Runner row (incl. device_id +
/// token_hash) on a token-prefix match. Caller is responsible for the
/// argon2 verify; we keep the hash in this struct only.
pub struct RunnerAuth {
    pub id: i64,
    pub device_id: String,
    pub token_hash: String,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct RunnerAuthRow {
    id: i64,
    device_id: String,
    token_hash: Vec<u8>,
    revoked_at: Option<DateTime<Utc>>,
}

pub async fn find_runner_for_auth(pool: &PgPool, token_prefix: &str) -> Result<Option<RunnerAuth>> {
    let row: Option<RunnerAuthRow> = sqlx::query_as(
        "SELECT id, device_id, token_hash, revoked_at FROM runners \
         WHERE token_prefix = $1",
    )
    .bind(token_prefix)
    .fetch_optional(pool)
    .await
    .context("lookup runner by token_prefix")?;
    Ok(row.map(|r| RunnerAuth {
        id: r.id,
        device_id: r.device_id,
        token_hash: String::from_utf8_lossy(&r.token_hash).into_owned(),
        revoked_at: r.revoked_at,
    }))
}

pub async fn touch_runner(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query("UPDATE runners SET last_used_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("touch runner")?;
    Ok(())
}

// ---- Case results --------------------------------------------------------

pub struct NewCaseResult<'a> {
    pub submission_id: Uuid,
    pub case_ord: i32,
    pub status: &'a str,
    pub exit_code: Option<i32>,
    pub cycles: Option<i64>,
    pub output: Option<&'a [u8]>,
    pub passed: Option<bool>,
    pub synthetic: bool,
}

pub async fn insert_case_result(pool: &PgPool, r: NewCaseResult<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO case_results \
         (submission_id, case_ord, status, exit_code, cycles, output, passed, synthetic) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT (submission_id, case_ord) DO UPDATE \
         SET status = EXCLUDED.status, \
             exit_code = EXCLUDED.exit_code, \
             cycles = EXCLUDED.cycles, \
             output = EXCLUDED.output, \
             passed = EXCLUDED.passed, \
             synthetic = EXCLUDED.synthetic",
    )
    .bind(r.submission_id)
    .bind(r.case_ord)
    .bind(r.status)
    .bind(r.exit_code)
    .bind(r.cycles)
    .bind(r.output)
    .bind(r.passed)
    .bind(r.synthetic)
    .execute(pool)
    .await
    .context("insert case_result")?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CaseResultRow {
    pub submission_id: Uuid,
    pub case_ord: i32,
    pub status: String,
    pub exit_code: Option<i32>,
    pub cycles: Option<i64>,
    pub output: Option<Vec<u8>>,
    pub passed: Option<bool>,
    pub synthetic: bool,
}

// ---- Leaderboards --------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LeaderboardRow {
    pub rank: i64,
    pub submission_id: Uuid,
    pub user_id: i64,
    pub github_login: String,
    pub avatar_url: Option<String>,
    pub total_cycles: i64,
    pub finished_at: DateTime<Utc>,
    pub synthetic: bool,
}

/// Per-(problem, board) leaderboard. Returns the top `limit` rows by lowest
/// `total_cycles`, plus the viewer's own row if they have a fully-passing
/// submission outside the top N. Per user, only the best submission counts.
///
/// Eligibility: status='done', total_cycles IS NOT NULL (i.e. every case OK),
/// passed = total_cases (every case matched expected_output).
pub async fn leaderboard(
    pool: &PgPool,
    problem_id: &str,
    board: &str,
    limit: i32,
    viewer_id: Option<i64>,
) -> Result<Vec<LeaderboardRow>> {
    // CTE shape:
    //   best     — per-user best eligible submission via DISTINCT ON
    //   ranked   — same rows + a global rank assigned by total_cycles ASC
    // Final filter keeps top-N plus the viewer's row regardless of rank, so
    // callers always know where they stand. NULLs in viewer_id collapse the
    // OR branch via $5::BIGINT IS NOT NULL.
    let rows = sqlx::query_as::<_, LeaderboardRow>(
        r#"
        WITH best AS (
            SELECT DISTINCT ON (s.user_id)
                s.id AS submission_id,
                s.user_id,
                s.total_cycles,
                s.finished_at,
                s.device_id
            FROM submissions s
            WHERE s.problem_id   = $1
              AND s.board        = $2
              AND s.status       = 'done'
              AND s.total_cycles IS NOT NULL
              AND s.passed       = s.total_cases
            ORDER BY s.user_id, s.total_cycles ASC, s.finished_at ASC
        ), ranked AS (
            SELECT b.*,
                   ROW_NUMBER() OVER (ORDER BY b.total_cycles ASC, b.finished_at ASC) AS rank
            FROM best b
        )
        SELECT r.rank,
               r.submission_id,
               r.user_id,
               u.github_login,
               u.avatar_url,
               r.total_cycles,
               r.finished_at,
               COALESCE(d.synthetic, false) AS synthetic
        FROM ranked r
        JOIN users u  ON u.id = r.user_id
        LEFT JOIN devices d ON d.id = r.device_id
        WHERE r.rank <= $3
           OR ($4::BIGINT IS NOT NULL AND r.user_id = $4)
        ORDER BY r.rank
        "#,
    )
    .bind(problem_id)
    .bind(board)
    .bind(limit as i64)
    .bind(viewer_id)
    .fetch_all(pool)
    .await
    .context("leaderboard query")?;
    Ok(rows)
}

pub async fn list_case_results(pool: &PgPool, submission_id: Uuid) -> Result<Vec<CaseResultRow>> {
    let rows = sqlx::query_as::<_, CaseResultRow>(
        "SELECT submission_id, case_ord, status, exit_code, cycles, output, passed, synthetic \
         FROM case_results WHERE submission_id = $1 ORDER BY case_ord",
    )
    .bind(submission_id)
    .fetch_all(pool)
    .await
    .context("list case_results")?;
    Ok(rows)
}
