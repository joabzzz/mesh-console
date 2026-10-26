# Bounded Qwen 27B desktop demo

This branch adds an opt-in **Private GPU demo** screen to the Tauri app. It uses
m8s's certified, model-free owner client, the existing Chat renderer and the live
mesh sidebar. It creates a dedicated demo network; it does not add a rental to
an arbitrary pre-existing desktop network.

The normal embedded-node and Goose flows remain available when demo mode is not
configured. Demo chat is a bounded, plain conversation through the pinned Qwen
model, with history held in memory. It does not invoke Goose tools. Answers are
displayed after the complete response and exact response-model validation.

## Prepare

Use the m8s repository's Qwen runbook to qualify the pinned native client and
image, account for prior spending/liabilities, and prepare a fresh private plan.
This desktop adapter permits at most USD5 of new allowance; the underlying plan
and m8s admission may impose a smaller limit. A cap is not a final bill.

The m8s CLI must include `m8s.desktop-owner/v1` attachment export. Start the
one-attempt daemon with the selected plan before opening the app:

```sh
/absolute/m8sd --vast-demo --state-dir /absolute/private/CONTROL \
  --plan /absolute/private/plan.json --m8s /absolute/m8s
```

This initializes the control socket and frozen plan but does not rent anything.
Create a mode-0600 JSON config inside a mode-0700 directory:

```json
{
  "cli": "/absolute/m8s",
  "control_directory": "/absolute/private/CONTROL"
}
```

Run the desktop with that explicit config (and an isolated Goose data directory):

```sh
M8S_DESKTOP_CONFIG=/absolute/private/desktop.json \
GOOSE_PATH_ROOT=/absolute/private/goose \
CARGO_BUILD_JOBS=2 bin/just run
```

`Add compute` requests one `m8s up qwen38-27b`. The coordinator authenticates its
local client, publishes only public identity and local ports in a private
attachment file, then performs the existing bounded provisioning workflow.
Credentials and private keys never enter the WebView. The adapter refuses
attachment files with unsafe modes, aliases or wrong ownership. It checks the
live owner's certificate identity and model-free role before exposing the node.

## Recording

1. Show the cap and choose **Add compute · Qwen 27B**.
2. Show the local certified client and GPU peer as observed; loading remains
   distinct from Ready. Ready requires the actual coordinator canary, exact
   model discovery and a live verified owner client.
3. Ask “Explain a mesh network in two sentences.” Follow with “How does adding
   this GPU help my laptop?” The backend preserves both turns and pins requests
   and responses to the exact model identities qualified by m8s.
4. Choose **Remove compute**. Wait for **GPU removed · provider confirmed**.
   This requires the coordinator's exact-absence event and matching private
   provider-removal receipt. Down acknowledgement alone is not removal.
5. Independently verify final provider inventories using the m8s runbook. Retain
   accounting journals and reservations. Capture only the dedicated app window.

Closing the desktop requests down; the m8s coordinator and independent watchdog
retain the original deadline even if the desktop crashes. Do not stop those
processes as a substitute for verified destruction. A consumed launch cannot be
replayed; a replacement needs a new admitted plan and confirmed prior absence.

## Validation

Run `cargo test --locked -j2 -p mesh-console`, strict Clippy, formatting, UI lint,
UI build and the mocked Playwright suite. Synthetic tests cover budget bounds,
identity/model substitution, private attachment files, stale readiness, and
removal confirmation. Mocked tests are rehearsal, not live GPU evidence.
