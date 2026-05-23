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

Step 6: end-to-end submissions against a Cortex-M device (QEMU or real
hardware). On submit, exact-api compiles the user snippet for
thumbv7m-none-eabi, packs the ELF into a monoexec `.bin` via
`monolink::pack_into`, and ships it over a runner WebSocket to a Pi-side
agent. The agent drives `monolink::Loader::upload_and_run` on the
device, streams per-case status / cycles / output back, and the API
persists everything to `case_results` for the frontend to render.

For QEMU dev mode, the runner spawns `qemu-system-arm` itself and uses
the PTY it advertises on stderr — the runner code path is the same as
real hardware otherwise. Cycle counts on QEMU are fabricated
deterministically per `(bin, case_input)` (siphash) so leaderboards
exercise sort-order code even without a real DWT.

### Verifying step 6 end-to-end

```sh
# Terminal 1: API
cargo run -p exact-api

# In the admin UI:
#   /admin/devices → register "qemu-local" (board lm3s6965evb,
#     cclk 12_000_000, synthetic on)
#   → provision a runner for it, copy the one-shot token to ./runner.token

# Terminal 2: build the mono-os kernel (one-time)
cd ../mono-os && cargo build --release -p kernel && cd -

# Terminal 3: runner
cargo run -p exact-runner -- \
  --backend-url ws://127.0.0.1:3000/api/runner/ws \
  --api-key-file ./runner.token \
  --device-id qemu-local \
  --qemu \
  --kernel ../mono-os/target/thumbv7m-none-eabi/release/kernel

# Frontend: visit /p/sum-to-n (or whichever problem you authored), hit
# Submit. Watch the case_results render with synthetic cycle counts.
```

See the parent plan at `/Users/asm/.claude/plans/in-this-folder-i-shiny-hare.md`
for the full implementation outline.

## License

MIT.
