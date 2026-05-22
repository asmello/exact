-- exact initial schema.
--
-- Mirrors the plan in /Users/asm/.claude/plans/in-this-folder-i-shiny-hare.md.
-- Subsequent migrations add: case_results.synthetic (already inline here),
-- and any later tables for run-time metadata.

CREATE TABLE users (
    id            BIGSERIAL    PRIMARY KEY,
    github_id     BIGINT       UNIQUE NOT NULL,
    github_login  TEXT         NOT NULL,
    avatar_url    TEXT,
    is_admin      BOOLEAN      NOT NULL DEFAULT false,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    id           TEXT         PRIMARY KEY,
    board        TEXT         NOT NULL,
    cclk_hz      BIGINT       NOT NULL,
    description  TEXT,
    active       BOOLEAN      NOT NULL DEFAULT true,
    last_seen    TIMESTAMPTZ,
    synthetic    BOOLEAN      NOT NULL DEFAULT false
);

CREATE TABLE runners (
    id            BIGSERIAL    PRIMARY KEY,
    device_id     TEXT         NOT NULL REFERENCES devices(id),
    label         TEXT         NOT NULL,
    token_hash    BYTEA        NOT NULL,
    token_prefix  TEXT         NOT NULL,
    created_by    BIGINT       NOT NULL REFERENCES users(id),
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    revoked_at    TIMESTAMPTZ,
    last_used_at  TIMESTAMPTZ
);
CREATE UNIQUE INDEX runners_token_prefix_idx ON runners (token_prefix);

CREATE TABLE problems (
    id                  TEXT         PRIMARY KEY,
    title               TEXT         NOT NULL,
    description_md      TEXT         NOT NULL,
    starter_code        TEXT         NOT NULL,
    io_spec             JSONB        NOT NULL,
    visibility          TEXT         NOT NULL,
    share_token         TEXT,
    default_timeout_ms  INTEGER      NOT NULL DEFAULT 100,
    allowed_boards      TEXT[]       NOT NULL,
    owner_id            BIGINT       NOT NULL REFERENCES users(id),
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE TABLE test_cases (
    id               BIGSERIAL    PRIMARY KEY,
    problem_id       TEXT         NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
    ord              INTEGER      NOT NULL,
    name             TEXT,
    input            BYTEA        NOT NULL,
    expected_output  BYTEA,
    weight           REAL         NOT NULL DEFAULT 1.0,
    hidden           BOOLEAN      NOT NULL DEFAULT false,
    UNIQUE (problem_id, ord)
);

CREATE TABLE submissions (
    id            UUID         PRIMARY KEY,
    user_id       BIGINT       NOT NULL REFERENCES users(id),
    problem_id    TEXT         REFERENCES problems(id),
    source_code   TEXT         NOT NULL,
    board         TEXT         NOT NULL,
    device_id     TEXT         REFERENCES devices(id),
    status        TEXT         NOT NULL,
    build_log     TEXT,
    bin_blob      BYTEA,
    total_cycles  BIGINT,
    passed        INTEGER,
    total_cases   INTEGER,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    finished_at   TIMESTAMPTZ
);
CREATE INDEX submissions_problem_board_cycles_done_idx
    ON submissions (problem_id, board, total_cycles) WHERE status='done';

CREATE TABLE case_results (
    submission_id  UUID         NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
    case_ord       INTEGER      NOT NULL,
    status         TEXT         NOT NULL,
    exit_code      INTEGER,
    cycles         BIGINT,
    output         BYTEA,
    passed         BOOLEAN,
    synthetic      BOOLEAN      NOT NULL DEFAULT false,
    PRIMARY KEY (submission_id, case_ord)
);
