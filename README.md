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

## Pi runner deployment

The Pi-side agent is a single binary cross-built on the dev box and
deployed under systemd. Three steps; the scripts in `scripts/` do the
heavy lifting.

```sh
# 1. On the dev box: cross-build for aarch64-linux (Pi 3/4/5, 64-bit OS).
#    Needs `nix develop` so cargo-zigbuild + zig are on PATH.
./scripts/build-runner-aarch64.sh
# → target/aarch64-unknown-linux-gnu/release/exact-runner

# 2. Provision a runner in the admin UI (/admin/devices) and copy the
#    one-shot token. Then ship the binary + token to the Pi:
scp target/aarch64-unknown-linux-gnu/release/exact-runner pi@bench.lan:
scp runner.token pi@bench.lan:
scp scripts/runner-install.sh scripts/exact-runner.service.in pi@bench.lan:

# 3. On the Pi: run the installer (it creates the service user, drops
#    the binary + token + systemd unit, and starts the service).
ssh pi@bench.lan
./runner-install.sh \
  --binary ./exact-runner \
  --backend-url wss://exact.run/api/runner/ws \
  --device-id lpc1768-pi-asm \
  --serial-port /dev/ttyACM0 \
  --token-file ./runner.token \
  --board lpc1768

systemctl status exact-runner
journalctl -u exact-runner -f
```

Verify on the API side: the device should turn green on `/admin/devices`
within a few seconds (`last_seen` ticks). Submit a problem against the
LPC1768 board — the case_results panel should show real cycle counts
(no `synth` badge).

The systemd unit is hardened: runs as a dedicated `exact-runner` user,
read-only filesystem (`ProtectSystem=strict`), no access to anything
outside the serial port device node (`DevicePolicy=closed` +
`DeviceAllow=<port> rw`), `MemoryDenyWriteExecute=true`. A compromised
runner can talk to the backend over WSS and drive the attached MCU and
nothing else.

## License

MIT.
