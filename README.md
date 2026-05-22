# exact

A LeetCode-style judge that compiles user-submitted Rust snippets, runs
them on real Cortex-M hardware via [`mono-os`](../mono-os), and reports
cycle-accurate per-case timing. The judging layer that sits in front of
mono-os's runtime.

This repo is the **frontend + service tier**. The Cortex-M runtime,
`userlib` shims, and `monolink` host loader live in the sibling
[`mono-os`](../mono-os) checkout. `exact` consumes that as a path
dependency during dev and as a Git dependency once the Railway Dockerfile
lands.

## Layout

```
exact/
├── crates/
│   ├── exact-api/         # axum HTTP + WS service deployed on Railway
│   ├── exact-runner/      # Pi-side agent driving one Cortex-M device
│   └── exact-proto/       # shared serde types (api <-> runner, api <-> frontend)
├── frontend/              # SvelteKit + Tailwind v4 + CodeMirror 6
├── flake.nix              # nightly rust + thumbv7m + node + pnpm + postgres
└── rust-toolchain.toml
```

## Dev loop

```sh
nix develop
# or `direnv allow` once for automatic shell activation

# Copy .env.example to .env (or use direnv) and fill in DATABASE_URL,
# GITHUB_CLIENT_ID/SECRET, SESSION_SECRET, etc. See `.env.example`.

# Postgres (one-time setup, then `pg_ctl start`/`stop` to manage):
initdb -D .pgdata
pg_ctl -D .pgdata -l .pgdata/log start
createdb exact

# Backend
cargo check --workspace
cargo run -p exact-api          # runs migrations on startup

# Frontend (separate terminal)
cd frontend
pnpm install
pnpm dev          # http://127.0.0.1:5173, proxies /api and /auth to :3000
pnpm check        # svelte-check + tsc
```

`pnpm dev`'s vite proxies `/api` and `/auth` to the api service on `:3000`,
so the SPA and the backend can be developed against the same browser
origin.

## Status

Step 3 of the implementation plan: Postgres schema + GitHub OAuth
sign-in + `/api/me`. Sessions are signed cookies (HMAC over user id),
so restarts don't log users out. The editor page renders Rust with
proper syntax highlighting via `@codemirror/theme-one-dark`.

No problems, submissions, or runner yet — those land in steps 4–6.

See the parent plan at `/Users/asm/.claude/plans/in-this-folder-i-shiny-hare.md`
for the full implementation outline.

## License

MIT.
