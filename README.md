# mesh-console

Desktop UI for operating a local [mesh-llm](https://github.com/mesh-llm/mesh-llm) node: status, mesh topology, model runtime, and chat against the node’s OpenAI-compatible proxy.

Built with [Dioxus](https://dioxuslabs.com/) 0.6 and Rust 2021. Inference chat uses the public Rust SDK (`mesh-api` from mesh-llm, pinned in `Cargo.toml`); dashboard and control paths talk to the node’s management HTTP API.

## Prerequisites

- **Rust** (stable), with `cargo` on your `PATH`.
- **`mesh-llm`** installed and available as the `mesh-llm` command. The console spawns `mesh-llm serve …` when you start or join a mesh from the UI.
- A **running mesh-llm daemon** on this machine. The app expects the management API at `http://127.0.0.1:3131` (status, models, runtime, SSE events). Chat streams through the SDK toward the proxy port reported in status (`api_port`, commonly `9337`).

If nothing is listening on `:3131`, the UI will show as disconnected until you start or attach to a node.

## Run from source

From this directory:

```bash
cargo run
```

The default feature set builds the **desktop** app (`desktop` is the default Cargo feature). Release binary:

```bash
cargo build --release
# binary: target/release/mesh-console
```

### Other targets

Cargo features (see `Cargo.toml`):

| Feature   | Purpose        |
|-----------|----------------|
| `desktop` | Native window (default). |
| `web`     | `dioxus/web` — build/serve per Dioxus web workflow. |
| `mobile`  | `dioxus/mobile` — mobile targets per Dioxus docs. |

Example (web only, no default features):

```bash
cargo build --no-default-features --features web
```

## What’s in the app

- **Home** — High-level status, shortcuts, start/join flows (delegates to `mesh-llm serve`).
- **Network** — Mesh/network dashboard fed by live status and events.
- **Chat** — Streaming chat via the mesh-llm Rust SDK against the local proxy (requires a mesh token and `api_port` from `/api/status`).
- **Models** — Catalog and runtime process view; complements the SSE-driven status refresh.

Status is kept fresh with a Server-Sent Events stream plus periodic runtime/process polling.

## Local data

| Path | Contents |
|------|----------|
| `<config_dir>/mesh-console/owner.hex` | Persisted owner keypair (hex). Created on first use; stable identity for SDK chat. |
| `<data_local_dir>/mesh-console/chats.json` | Saved chat sessions (desktop). |

`config_dir` / `data_local_dir` are the usual OS-specific directories (see the [`dirs`](https://docs.rs/dirs/) crate).

## SDK version

`mesh-api` is pulled from `https://github.com/mesh-llm/mesh-llm` at a **fixed git tag** in `Cargo.toml` so builds stay reproducible. Bump that tag deliberately when you want to adopt a newer SDK.

## Project layout

- `src/main.rs` — App shell, navigation, SSE + polling loop.
- `src/api.rs` — HTTP client for `http://127.0.0.1:3131` management routes.
- `src/sdk_session.rs` — SDK-backed chat and invite/keypair helpers.
- `src/process.rs` — Spawning and stopping `mesh-llm` child processes.
- `src/views/` — Home, dashboard/network, chat, models, topology.
- `assets/main.css` — Styles bundled via Dioxus `asset!`.
