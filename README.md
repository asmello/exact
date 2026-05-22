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

# Backend
cargo check --workspace
cargo run -p exact-api

# Frontend (separate terminal)
cd frontend
pnpm install
pnpm dev          # http://127.0.0.1:5173, proxies /api to localhost:3000
pnpm check        # svelte-check + tsc
```

`pnpm dev`'s vite proxies `/api` to the api service on `:3000`, so the
SPA and the backend can be developed independently.

## Status

Step 2 of the implementation plan: workspace + crate skeletons compile,
the SvelteKit dev server serves an editor page with CodeMirror 6 + Rust
syntax highlighting. No routes, no DB, no runner yet — those land in
subsequent steps.

See the parent plan at `/Users/asm/.claude/plans/in-this-folder-i-shiny-hare.md`
for the full implementation outline.

## License

MIT.
