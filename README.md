# mesh-console

A Dioxus desktop app that helps people start, stop, monitor, and chat with
`mesh-llm` instances on their machines. The dashboard mirrors the data the
embedded web UI at `http://127.0.0.1:3131/` shows; chat goes through the
public Rust SDK so we exercise the same surface third-party integrators use.

## Architecture: hybrid (management HTTP + SDK)

The mesh-llm side splits into two surfaces, and mesh-console talks to both:

```text
                     ┌────────────────────────────────────────────┐
                     │              mesh-llm daemon               │
                     ├────────────────────────┬───────────────────┤
 dashboard / models  │   management API       │   OpenAI proxy    │  inference
 topology / runtime  │   :3131  (/api/*)      │   :api_port       │  via SDK
                     └─────────▲──────────────┴────────▲──────────┘
                               │                       │
                  src/api.rs   │                       │  src/sdk_session.rs
                  reqwest+SSE  │                       │  mesh_api::MeshClient
                               │                       │
                     ┌─────────┴───────────────────────┴──────────┐
                     │              mesh-console                  │
                     └────────────────────────────────────────────┘
```

### Management HTTP on `:3131` (dashboard truth)

[`src/api.rs`](src/api.rs) keeps using the operator-facing endpoints. The
SDK does not expose any of these today, so dashboard, topology, models, and
runtime control all stay on raw HTTP:

- `GET /api/status` — node, peers, GPUs, routing, `local_instances`, etc.
- `GET /api/events` — Server-Sent Events stream of `MeshStatus` snapshots.
- `GET /api/models` — mesh model catalog (intentionally **not** in `/api/status`).
- `GET /api/runtime/processes` — live backend processes with PID and port.
- `POST /api/runtime/models` / `DELETE /api/runtime/models/{name}` — load / unload.

The status loop in [`src/main.rs`](src/main.rs) prefers SSE and only refetches
`/api/models` when a status-derived "catalog key" changes, mirroring the
embedded React UI. Polling falls back automatically with exponential backoff.

`mesh-llm serve` is started via [`src/process.rs`](src/process.rs) using the
local `mesh-llm` binary. Per-instance stop uses the `pid` reported by
`local_instances` rather than `pkill`, so it is safe with multiple daemons.

### Inference via the SDK ([`mesh_api`])

[`src/sdk_session.rs`](src/sdk_session.rs) builds a
[`mesh_api::MeshClient`](../mesh-llm/mesh-api/src/client.rs) per chat send and
streams the answer back into the UI:

- `OwnerKeypair` is persisted as hex at `<config_dir>/mesh-console/owner.hex`
  so the same identity is reused across launches.
- `InviteToken` comes from `MeshStatus.token` (the `/api/status` field).
- The OpenAI proxy URL is forwarded through `MESH_CLIENT_API_BASE` and points
  at `http://127.0.0.1:{MeshStatus.api_port}` (default 9337). That is
  deliberately **not** the management port `:3131`.

#### Why we go through `api_port`, not `:3131/api/chat`

The management proxy in
[`mesh-llm/src/api/routes/chat.rs`](../mesh-llm/mesh-llm/src/api/routes/chat.rs)
returns `503 LLM not ready` whenever the local node is **not** a client and
`llama_ready` is false:

```rust
if !inner.llama_ready && !inner.is_client {
    return respond_error(stream, 503, "LLM not ready").await;
}
```

That gate fires even when the OpenAI proxy on `api_port` is up and could
either serve locally or route to a peer. Talking to `api_port` directly via
the SDK matches what `mesh-client` ships and lets us debug:

- Host node, model loading: SDK works as soon as the proxy can route.
- Two-node setup where the host is "warm" but the second node thinks it is
  cold: the SDK on the cold node can still POST to its local proxy and the
  request is forwarded over the mesh.

If a request still fails, the error from the SDK (or the `Failed` event)
points at the real cause (proxy down, model not loaded, mesh unreachable)
rather than the catch-all 503.

### Today's SDK limits (good to know before debugging)

The client stack lives under [`mesh-llm/mesh-client`](../mesh-llm/mesh-client)
(Cargo package name `mesh-llm-client`, Rust import `mesh_client`). That is
**not** the top-level [`mesh-llm-client/`](../mesh-llm-client) folder in this
workspace, which is a separate Dioxus app; mesh-console never depends on it.

That SDK ships a non-streaming chat that hardcodes `max_tokens: 64` and
`temperature: 0`
([`builder.rs`](../mesh-llm/mesh-client/src/client/builder.rs)). Each
`Event::TokenDelta` is therefore the full reply, not an incremental token,
and answers are short. We surface what the SDK gives us so any future
streaming / parameter work in mesh-llm shows up here automatically.

`mesh_api::Status` only carries `{ connected, peer_count }`, so dashboard
data still has to come from `:3131`. Removing the management dependency
entirely is upstream work in mesh-llm, not in this app.

## Development

```bash
dx serve --platform desktop
```

```text
src/
├─ api.rs              # HTTP client for the :3131 management API + SSE
├─ sdk_session.rs      # mesh_api::MeshClient bridge for chat (api_port + token)
├─ process.rs          # mesh-llm serve / join / stop helpers
├─ state.rs            # serde types matching the management JSON
├─ status_display.rs   # peer status / model / VRAM helpers (mirrors React UI)
├─ main.rs             # SSE-driven status loop, app shell, navigation
├─ views/
│  ├─ dashboard.rs     # Network view (GPUs, model control, routing affinity)
│  ├─ topology.rs      # CSS-3D rotating mesh + node detail card + invite QR
│  ├─ chat.rs          # Chat UI backed by sdk_session::stream_chat
│  └─ models.rs        # Models, running processes, daemon instances
└─ components/         # Small shared UI bits
```
