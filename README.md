# mesh-console

Desktop app that turns a machine into a [mesh-llm](https://github.com/mesh-llm/mesh-llm) host: create a private AI network, join others over invite tokens or QR codes, host open models, and chat — **no CLI install required**. The entire mesh runtime runs in-process via the `mesh-llm-sdk` embedded node.

Built with [Dioxus](https://dioxuslabs.com/) 0.6 and Rust 2021.

## How it works

```
┌──────────────────────── mesh-console.app ────────────────────────┐
│  Dioxus UI (crates/console-app)                                   │
│    │ commands / phase updates                                     │
│  NodeManager (crates/console-core)                                │
│    └─ embedded mesh-llm node (mesh-llm-sdk "serving", own thread) │
│         ├─ management API  http://127.0.0.1:3131  (/api/*)        │
│         └─ OpenAI proxy    http://127.0.0.1:9337/v1               │
└───────────────────────────────────────────────────────────────────┘
```

- The UI drives the node through an in-process handle (start / join / stop / invite token) and reads **all data over the same management HTTP API the mesh-llm web console uses** — so the two stay in lockstep.
- First run: an onboarding wizard detects your hardware, downloads the native inference runtime (cached in `~/.cache/mesh-llm`), lets you pick a starter model, and creates your network in one click.
- If another mesh-llm daemon already owns the default ports, the app says so and offers to start on alternate ports (it never kills external processes).
- Quitting the app stops the embedded node cleanly.

## Run from source

```bash
cargo run -p mesh-console          # debug
just run                           # same
just bundle                        # release .app via `dx bundle`
```

## What's in the app

- **Onboarding** — first-run wizard: hardware detection, runtime download with progress, starter-model picker, one-click create/join, QR invite on success.
- **Home** — active mesh on this device, public networks discovered over Nostr, create/join flows.
- **Network** — live 2D/3D topology, model control, GPU bars, peers table (state, latency + source, VRAM, version, owner/attestation badges), inference-runtime slots & metrics, routing stats.
- **Chat** — streams through `/api/responses` like the web console: model select (incl. `auto` mesh routing), attachments (images/PDF/audio), collapsible reasoning, token usage + TTFT/total timings, served-by peer. Sessions persist locally.
- **Reserves** — wakeable nodes grouped by provider with state, wake ETA, and capacity summaries.
- **Configuration** — owner-gated runtime config with undo/redo and revision-checked apply: defaults, deployment, signing/admission, plugin toggles, and a TOML editor validated by the real mesh-llm config parser.
- **Models** — mesh catalog (warm/cold, quantization, capabilities, context length, fit) with load/unload and expandable detail.
- **Invite** (header) — copy `mesh-llm --join …` commands, raw token, OpenAI API URL, and the QR code.
- **Settings** — owner identity, node defaults, data locations.

## Testing

```bash
just test        # unit + mock-server tests (fast; no inference runtime compiled)
just test-node   # real embedded node lifecycle smoke test (no downloads)
just test-heavy  # serve-mode + model download + chat round-trip (opt-in)
just lint        # clippy -D warnings + fmt --check
```

- `crates/console-types` mirrors the management-API payloads (`mesh-llm-ui`'s `types.ts` is the cross-check); fixture round-trip tests are the drift alarm. Re-capture fixtures from a live node after SDK bumps:
  `cargo test -p console-core --test capture_fixtures -- --ignored`
- CI (`.github/workflows/ci.yml`) runs lint + fast tests + the embedded smoke test on every push, and the heavy serve/chat test nightly.

## Local data

| Path | Contents |
|------|----------|
| `<config_dir>/mesh-console/owner.hex` | Owner keypair (hex) — stable identity for chat. |
| `<config_dir>/mesh-console/owner-keystore.json` | Same identity in mesh-llm keystore format, used by the embedded node (owner-verified status). |
| `<data_local_dir>/mesh-console/chats.json` | Saved chat sessions. |
| `<data_local_dir>/mesh-console/settings.json` | Last node setup (also marks onboarding as done). |
| `~/.cache/mesh-llm/` | Native inference runtime + downloaded models (shared with the CLI). |

## Workspace layout

| Crate | Purpose |
|-------|---------|
| `crates/console-types` | Tolerant serde mirrors of the management-API payloads. Zero heavy deps. |
| `crates/console-core` | `ApiClient` (HTTP + SSE + responses streaming + control), `NodeManager` (embedded node lifecycle), view-model logic (`vm/`), persistence. The `embedded` feature (default) pulls the full mesh runtime; disable it for fast logic-only builds/tests. |
| `crates/console-app` | The Dioxus desktop UI. |

## SDK version

`mesh-llm-sdk` (plus `mesh-llm-identity` / `mesh-llm-config`) is pinned to an exact git revision of `mesh-llm` in the workspace `Cargo.toml` — the embedded-serving facade isn't in a tagged release yet. Bump deliberately, then re-capture fixtures. For local development against the sibling `../mesh-llm` checkout, uncomment the `[patch]` block in `Cargo.toml`.
