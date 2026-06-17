#!/usr/bin/env python3
"""Visual test harness: render every app screen as static HTML over the real
stylesheet and screenshot it with headless Chrome — no screen-recording
permission or running daemon needed.

    python3 ux/build_previews.py            # build HTML + shoot light/dark PNGs
    python3 ux/build_previews.py --html     # build HTML only

The DOM here mirrors the rsx in crates/console-app/src; when a view's
structure changes meaningfully, update its snippet. Shots land in ux/shots/.
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CSS = ROOT / "crates/console-app/assets/main.css"
PREVIEWS = ROOT / "ux/previews"
SHOTS = ROOT / "ux/shots"
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

# Re-declare tokens explicitly per variant: headless Chrome inherits the OS
# color scheme, so both themes must be forced for deterministic shots.
THEME_OVERRIDES = {
    "light": """
:root {
  --paper: #ffffff; --paper-2: #f6f6f6; --paper-3: #ededed;
  --ink: #111111; --ink-2: #555555; --ink-3: #9b9b9b;
  --line: #e4e4e4; --line-2: #c9c9c9;
  --ink-on-ink: #ffffff; --scrim: rgba(17, 17, 17, 0.45);
}
""",
    "dark": """
:root {
  --paper: #0e0e0e; --paper-2: #161616; --paper-3: #1e1e1e;
  --ink: #f2f2f2; --ink-2: #a8a8a8; --ink-3: #6e6e6e;
  --line: #262626; --line-2: #3d3d3d;
  --ink-on-ink: #0e0e0e; --scrim: rgba(0, 0, 0, 0.6);
}
""",
}

TOPNAV = """
<header class="topnav">
  <div class="topnav-left">
    <span class="topnav-brand">
      <span class="topnav-brand-icon"></span>
      <span class="topnav-brand-name">mesh</span><span class="topnav-brand-llm">llm</span>
    </span>
    <div class="topnav-tabs">
      <button class="topnav-tab">Home</button>
      <button class="topnav-tab topnav-tab--active">Network</button>
      <button class="topnav-tab">Chat</button>
      <button class="topnav-tab">Reserves</button>
    </div>
  </div>
  <div class="topnav-right">
    <button class="btn btn--ghost btn--sm topnav-invite">Invite</button>
    <button class="topnav-icon-btn"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><line x1="2" y1="4" x2="14" y2="4"/><line x1="2" y1="8" x2="14" y2="8"/><line x1="2" y1="12" x2="14" y2="12"/><circle cx="10" cy="4" r="1.6" fill="var(--paper)"/><circle cx="5" cy="8" r="1.6" fill="var(--paper)"/><circle cx="12" cy="12" r="1.6" fill="var(--paper)"/></svg></button>
    <button class="topnav-icon-btn topnav-icon-btn--active"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="5" height="5"/><rect x="9" y="2" width="5" height="5"/><rect x="2" y="9" width="5" height="5"/><rect x="9" y="9" width="5" height="5"/></svg></button>
  </div>
</header>
"""

PAGES = {}

PAGES["onboarding-welcome"] = """
<div class="view onboarding">
  <div class="onboarding-card">
    <div class="onboarding-step"><span>step 1 of 3</span>
      <div class="onboarding-step-bar"><div class="onboarding-step-bar-fill" style="width:16%"></div></div>
    </div>
    <h1 class="onboarding-title">Your hardware. Your models. Your network.</h1>
    <p class="onboarding-lede">mesh-console turns this machine into a private AI network: host open models locally, invite other machines, and use it all through one OpenAI-compatible endpoint. No cloud. No accounts.</p>
    <div class="onboarding-hw">
      <span class="badge badge--info">Apple M4 Pro</span>
      <span class="badge badge--dim">48 GB memory</span>
      <span class="badge badge--dim">unified</span>
    </div>
    <div class="onboarding-paths">
      <button class="onboarding-path">Create my network</button>
      <button class="onboarding-path">Join an existing one</button>
    </div>
    <button class="btn--ghost btn--sm onboarding-skip">Skip — take me to the console</button>
  </div>
</div>
"""

PAGES["onboarding-model"] = """
<div class="view onboarding">
  <div class="onboarding-card">
    <div class="onboarding-step"><span>step 2 of 3</span>
      <div class="onboarding-step-bar"><div class="onboarding-step-bar-fill" style="width:50%"></div></div>
    </div>
    <h1 class="onboarding-title">Pick your first model.</h1>
    <p class="onboarding-lede">Sized to this machine — you can add or swap models any time.</p>
    <div class="onboarding-models">
      <button class="onboarding-model onboarding-model--active">
        <span class="onboarding-model-name">GLM 4.7 Flash</span>
        <span class="onboarding-model-blurb">Fast mixture-of-experts — the best all-rounder</span>
        <span class="onboarding-model-size">17.6 GB</span>
      </button>
      <button class="onboarding-model">
        <span class="onboarding-model-name">Qwen3 32B</span>
        <span class="onboarding-model-blurb">Top quality — needs lots of memory</span>
        <span class="onboarding-model-size">19.8 GB</span>
      </button>
      <button class="onboarding-model">
        <span class="onboarding-model-name">Qwen3 14B</span>
        <span class="onboarding-model-blurb">Serious quality for bigger machines</span>
        <span class="onboarding-model-size">9.0 GB</span>
      </button>
      <button class="onboarding-model">
        <span class="onboarding-model-name">Qwen3 0.6B</span>
        <span class="onboarding-model-blurb">Tiny and instant — great for trying the mesh</span>
        <span class="onboarding-model-size">0.5 GB</span>
      </button>
    </div>
    <div class="field">
      <label class="field-label">network name — optional</label>
      <input class="field-input" placeholder="e.g. den-mesh">
    </div>
    <button class="btn onboarding-cta">Create my private network</button>
    <button class="btn--ghost btn--sm onboarding-skip">Back</button>
  </div>
</div>
"""

PAGES["onboarding-working"] = """
<div class="view onboarding">
  <div class="onboarding-card">
    <div class="onboarding-step"><span>step 2 of 3</span>
      <div class="onboarding-step-bar"><div class="onboarding-step-bar-fill" style="width:80%"></div></div>
    </div>
    <h1 class="onboarding-title">Downloading the inference engine…</h1>
    <p class="onboarding-lede">One-time download, cached for every future launch.</p>
    <div class="notice notice--info node-phase">
      <div class="node-phase-title">Downloading inference runtime…</div>
      <div class="node-phase-detail">412 MB / 1.02 GB</div>
      <div class="progress-track"><div class="progress-fill" style="width:40%"></div></div>
      <div class="node-phase-hint">One-time download — the runtime is cached for future launches.</div>
    </div>
  </div>
</div>
"""

PAGES["onboarding-success"] = """
<div class="view onboarding">
  <div class="onboarding-card onboarding-card--success">
    <div class="onboarding-step"><span>live</span>
      <div class="onboarding-step-bar"><div class="onboarding-step-bar-fill" style="width:100%"></div></div>
    </div>
    <h1 class="onboarding-title">Your network is live.</h1>
    <p class="onboarding-lede">This machine is now a host. Other machines join by scanning the code — phones, laptops, anything that can run mesh-llm.</p>
    <div class="qr-frame qr-frame--inline"><div class="qr-svg"><svg viewBox="0 0 29 29" width="200" height="200"><rect width="29" height="29" fill="#fff"/><path fill="#0c0d10" d="M1 1h7v7H1zM21 1h7v7h-7zM1 21h7v7H1zM3 3h3v3H3zM23 3h3v3h-3zM3 23h3v3H3zM10 1h2v2h-2zM14 2h2v3h-2zM18 1h1v4h-1zM10 5h3v2h-3zM16 6h3v2h-3zM1 10h3v2H1zM5 11h4v2H5zM10 10h2v4h-2zM13 12h4v2h-4zM19 10h3v3h-3zM24 10h4v2h-4zM2 14h2v4H2zM6 15h3v2H6zM12 16h2v3h-2zM16 15h2v2h-2zM20 14h2v4h-2zM24 14h2v2h-2zM27 15h1v4h-1zM10 20h3v2h-3zM14 21h2v2h-2zM17 19h3v3h-3zM10 24h2v4h-2zM13 25h4v2h-4zM21 21h2v2h-2zM24 22h3v2h-3zM21 25h3v3h-3zM26 26h2v2h-2z"/></svg></div></div>
    <div class="token-row">
      <code class="token-value">mesh1qfzv9k3m7w2e8u4r6t1y5a9s0d2f4g6h8j0k2l4z6x8c0v…</code>
      <button class="btn btn--ghost btn--sm">copy</button>
    </div>
    <p class="onboarding-hint">Any OpenAI-compatible app can use it at <code>http://127.0.0.1:9337/v1</code></p>
    <button class="btn onboarding-cta">Open the console</button>
  </div>
</div>
"""

PAGES["network"] = TOPNAV + """
<main class="topnav-content"><div class="view view--wide">
  <header class="view-header">
    <h1 class="view-title">peanut</h1>
    <span class="badge badge--dim">host</span>
    <span class="badge badge--dim">Serving</span>
    <span class="badge badge--info">mesh · den-mesh</span>
  </header>
  <div class="topology-stage topology-stage--3d" style="height:340px">
    <div class="topology-controls">
      <div class="topology-mode-toggle">
        <button class="topology-mode-btn">2D</button>
        <button class="topology-mode-btn topology-mode-btn--active">3D</button>
      </div>
      <button class="btn btn--ghost btn--sm">QR invite</button>
    </div>
    <div style="position:absolute;left:50%;top:54%;transform:translate(-50%,-50%)">
      <div style="width:64px;height:64px;background:linear-gradient(145deg,#fff 0%,#f6f7f9 38%,#e2e5eb 72%,#cfd4dd 100%);box-shadow:0 0 32px rgba(255,255,255,.9),0 0 72px rgba(255,255,255,.6);border:1px solid #fff"></div>
    </div>
    <div style="position:absolute;left:24%;top:42%"><div style="width:34px;height:34px;background:linear-gradient(135deg,#6e6e6e,#3c3c3c);border:1px solid rgba(255,255,255,.6)"></div></div>
    <div style="position:absolute;left:72%;top:38%"><div style="width:26px;height:26px;background:linear-gradient(135deg,#202020,#121212);border:1px solid rgba(255,255,255,.25)"></div></div>
    <div class="topology-3d-ground" style="--topology-orbit-r:200px"></div>
  </div>
  <section class="section">
    <h2 class="section-title">Model</h2>
    <div class="info-card">
      <div class="info-row"><span class="info-label">status</span><span class="badge badge--green">online</span><span class="row-meta" style="margin-left:8px">17.6 GB on disk</span></div>
      <div class="info-row"><span class="info-label">model</span><span class="info-value">GLM-4.7-Flash-Q4_K_M</span></div>
    </div>
  </section>
  <section class="section">
    <h2 class="section-title">Inference runtime</h2>
    <div class="stat-grid stat-grid--compact">
      <div class="stat-cell"><span class="stat-label">slots</span><span class="stat-value">1 / 4</span></div>
      <div class="stat-cell"><span class="stat-label">prompt tokens</span><span class="stat-value">5,231</span></div>
      <div class="stat-cell"><span class="stat-label">generated tokens</span><span class="stat-value">1,874</span></div>
      <div class="stat-cell"><span class="stat-label">kv cache</span><span class="stat-value">42%</span></div>
    </div>
    <div class="slot-row">
      <span class="slot-dot slot-dot--busy"></span><span class="slot-dot"></span><span class="slot-dot"></span><span class="slot-dot"></span>
    </div>
  </section>
  <section class="section">
    <div class="section-header-row"><h2 class="section-title">Peers</h2><span class="row-meta">3 nodes</span></div>
    <div class="peers-table">
      <div class="peers-table-head"><span>node</span><span>state</span><span>latency</span><span>vram</span><span>version</span><span>models</span></div>
      <div class="peers-table-row peers-table-row--self">
        <span class="peers-cell-name">peanut <span class="badge badge--info">this device</span> <span class="badge badge--green">✓</span></span>
        <span><span class="badge badge--green">serving</span></span>
        <span class="row-meta">—</span><span class="row-meta">48 GB</span><span class="row-meta">v0.72.0</span>
        <span class="peers-cell-models"><span class="chip chip--good">GLM-4.7-Flash</span></span>
      </div>
      <div class="peers-table-row">
        <span class="peers-cell-name">studio <span class="badge badge--green">✓</span> <span class="badge badge--dim">signed</span><span class="row-meta peers-cell-gpu">Apple M2 Max</span></span>
        <span><span class="badge badge--green">serving</span></span>
        <span class="row-meta">11 ms</span><span class="row-meta">32 GB</span><span class="row-meta">v0.72.0</span>
        <span class="peers-cell-models"><span class="chip chip--good">Qwen3-14B</span></span>
      </div>
      <div class="peers-table-row">
        <span class="peers-cell-name">laptop-air</span>
        <span><span class="badge badge--info">client</span></span>
        <span class="row-meta">3 ms <span class="peers-latency-source">(est.)</span></span><span class="row-meta">0 GB</span><span class="row-meta">v0.71.0</span>
        <span class="peers-cell-models"></span>
      </div>
    </div>
  </section>
  <div class="stat-grid">
    <div class="stat-cell"><span class="stat-label">vram in use</span><span class="stat-value">17.6 / 48 GB</span></div>
    <div class="stat-cell"><span class="stat-label">in-flight</span><span class="stat-value">2</span></div>
    <div class="stat-cell"><span class="stat-label">api port</span><span class="stat-value">9337</span></div>
    <div class="stat-cell"><span class="stat-label">version</span><span class="stat-value">0.72.0</span></div>
  </div>
</div></main>
"""

PAGES["chat"] = TOPNAV + """
<main class="topnav-content"><div class="view view--wide chat-view chat-view--layout">
  <section class="section chat-view-header">
    <h2 class="section-title">Chat</h2>
    <p class="chat-lede">Streams through <code>/api/responses</code> · <code>api :9337</code> · <span class="badge badge--dim">host</span> <span class="badge badge--green" style="margin-left:6px">llama_ready</span></p>
  </section>
  <div class="chat-layout">
    <aside class="chat-sidebar">
      <button class="btn btn--sm chat-sidebar-new">New chat</button>
      <div class="chat-sidebar-list">
        <div class="chat-session chat-session--active"><span class="chat-session-title">What is a mesh network?</span><button class="chat-session-del">×</button></div>
        <div class="chat-session"><span class="chat-session-title">Summarize this PDF</span><button class="chat-session-del">×</button></div>
      </div>
    </aside>
    <div class="chat-main">
      <section class="section chat-controls chat-controls--inline">
        <label class="field-label">Model</label>
        <select class="field-input chat-model-select"><option>auto</option></select>
      </section>
      <div class="chat-scroll">
        <div class="chat-row chat-row--user"><div class="chat-bubble chat-bubble--user"><pre class="chat-bubble-text">What is a mesh network, in one sentence?</pre></div></div>
        <div class="chat-row chat-row--assistant"><div class="chat-bubble chat-bubble--assistant">
          <details class="chat-reasoning" open><summary class="chat-reasoning-summary">Reasoning</summary><pre class="chat-reasoning-text">The user wants a single-sentence definition…</pre></details>
          <pre class="chat-bubble-text">A mesh network is a set of devices that connect directly to each other, sharing capacity so the whole network keeps working even when any single node drops out.</pre>
          <div class="chat-meta-row"><span class="chat-meta">12↑ 38↓ tokens · ttft 118 ms · total 1.4 s</span><span class="badge badge--dim chat-meta-served">via studio</span></div>
        </div></div>
      </div>
      <div class="chat-composer">
        <textarea class="chat-input" placeholder="Message…"></textarea>
        <div class="chat-composer-bar">
          <span class="chat-composer-hint"><kbd class="chat-kbd">Enter</kbd> send · <kbd class="chat-kbd">Shift+Enter</kbd> newline</span>
          <div class="chat-composer-actions">
            <button class="btn btn--ghost btn--sm">+ Attach</button>
            <button class="btn btn--ghost btn--sm">Clear</button>
            <button class="btn">Send</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</div></main>
"""

PAGES["config"] = TOPNAV + """
<main class="topnav-content"><div class="view view--wide">
  <header class="view-header">
    <h1 class="view-title">Configuration</h1>
    <span class="badge badge--dim">revision 7</span>
    <span class="badge badge--warn">unsaved changes</span>
  </header>
  <div class="config-tabs">
    <button class="config-tab config-tab--active">Defaults</button>
    <button class="config-tab">Deployment</button>
    <button class="config-tab">Wake Policy</button>
    <button class="config-tab">Signing</button>
    <button class="config-tab">Integrations</button>
    <button class="config-tab">TOML Review</button>
  </div>
  <section class="section">
    <h2 class="section-title">Inference defaults</h2>
    <p class="empty-hint">Apply to every model unless a model entry overrides them.</p>
    <div class="config-grid">
      <div class="field config-field"><label class="field-label">context size</label><input class="field-input" value="8192"><span class="field-hint">tokens per slot</span></div>
      <div class="field config-field"><label class="field-label">parallel slots</label><input class="field-input" placeholder="auto"><span class="field-hint">concurrent requests</span></div>
      <div class="field config-field"><label class="field-label">tuning profile</label><input class="field-input" value="balanced"><span class="field-hint">latency | balanced | throughput</span></div>
      <div class="field config-field"><label class="field-label">KV cache type (K)</label><input class="field-input" value="f16"><span class="field-hint">f16 | q8_0 | q4_0</span></div>
    </div>
  </section>
  <div class="config-apply-bar">
    <button class="btn btn--ghost btn--sm">Undo</button>
    <button class="btn btn--ghost btn--sm" disabled>Redo</button>
    <button class="btn btn--ghost btn--sm">Discard</button>
    <button class="btn">Apply</button>
  </div>
</div></main>
"""

PAGES["invite-modal"] = TOPNAV + """
<main class="topnav-content"><div class="view view--wide"><header class="view-header"><h1 class="view-title">Network</h1></header></div></main>
<div class="modal-backdrop">
  <div class="modal-card modal-card--wide">
    <div class="modal-head"><h3 class="modal-title">Invite to this mesh</h3><button class="btn btn--ghost btn--sm">close</button></div>
    <p class="modal-hint">Invite with a model loaded to add compute, or invite as a client for API-only access.</p>
    <div class="qr-frame qr-frame--inline"><div class="qr-svg"><svg viewBox="0 0 29 29" width="200" height="200"><rect width="29" height="29" fill="#fff"/><path fill="#0c0d10" d="M1 1h7v7H1zM21 1h7v7h-7zM1 21h7v7H1zM3 3h3v3H3zM23 3h3v3h-3zM3 23h3v3H3zM10 1h2v2h-2zM14 2h2v3h-2zM18 1h1v4h-1zM10 5h3v2h-3zM16 6h3v2h-3zM1 10h3v2H1zM5 11h4v2H5zM10 10h2v4h-2zM13 12h4v2h-4zM19 10h3v3h-3zM24 10h4v2h-4z"/></svg></div></div>
    <div class="invite-block">
      <div class="invite-block-title">Contribute compute <span class="badge badge--green">recommended</span></div>
      <div class="token-row"><code class="token-value">mesh-llm --join mesh1qfzv9k… --model GLM-4.7-Flash-Q4_K_M</code><button class="btn btn--ghost btn--sm">copy</button></div>
    </div>
    <div class="invite-block">
      <div class="invite-block-title">Join as client</div>
      <div class="token-row"><code class="token-value">mesh-llm --client --join mesh1qfzv9k…</code><button class="btn btn--ghost btn--sm">copy</button></div>
    </div>
    <div class="invite-block">
      <div class="invite-block-title">API access</div>
      <div class="token-row"><code class="token-value">http://127.0.0.1:9337/v1</code><button class="btn btn--ghost btn--sm">copy</button></div>
    </div>
  </div>
</div>
"""

PAGES["home"] = TOPNAV.replace('topnav-tab topnav-tab--active">Network', 'topnav-tab">Network').replace('topnav-tab">Home', 'topnav-tab topnav-tab--active">Home') + """
<main class="topnav-content"><div class="view view--wide">
  <header class="view-header">
    <h1 class="view-title">Home</h1>
    <span class="badge badge--dim">owner · 332fe7fc22415…</span>
  </header>
  <section class="section">
    <div class="section-header-row"><h2 class="section-title">Active on this device</h2></div>
    <div class="row-list">
      <div class="row-item row-item--mesh row-item--clickable">
        <div class="row-main">
          <div class="peer-header"><span class="row-name">den-mesh</span><span class="badge badge--info">this device</span><span class="badge badge--dim">private</span></div>
          <div class="peer-meta"><span class="row-meta">2 peers</span><span class="row-meta">id <code>ae2ccea92d2b</code></span><span class="row-meta row-meta--hint">click to open topology →</span></div>
          <div class="cap-row"><span class="badge badge--green">GLM-4.7-Flash-Q4_K_M</span></div>
        </div>
        <div class="row-actions"><button class="btn btn--ghost btn--sm btn--danger">Stop</button></div>
      </div>
    </div>
  </section>
  <section class="section">
    <div class="section-header-row"><h2 class="section-title">Public networks</h2><button class="btn btn--ghost btn--sm">refresh</button></div>
    <div class="row-list">
      <div class="row-item row-item--mesh row-item--clickable">
        <div class="row-main">
          <div class="peer-header"><span class="row-name">commons</span><span class="badge badge--dim">us-west</span></div>
          <div class="peer-meta"><span class="row-meta">14 nodes</span><span class="row-meta">312 GB VRAM</span><span class="row-meta">5 clients</span></div>
          <div class="cap-row"><span class="badge badge--dim">Qwen3-32B</span><span class="badge badge--dim">GLM-4.7-Flash</span></div>
        </div>
        <div class="row-actions"><button class="btn btn--ghost btn--sm">Join</button></div>
      </div>
    </div>
  </section>
  <section class="section">
    <div class="section-header-row"><h2 class="section-title">Start a new mesh</h2><button class="home-fab">+</button></div>
  </section>
</div></main>
"""

PAGE_TEMPLATE = """<!doctype html>
<html><head><meta charset="utf-8">
<link rel="stylesheet" href="{css}">
{dark}
</head><body><div class="app app--topnav">{body}</div></body></html>
"""


def build(shoot: bool) -> None:
    PREVIEWS.mkdir(parents=True, exist_ok=True)
    SHOTS.mkdir(parents=True, exist_ok=True)
    css_href = CSS.resolve().as_uri()
    for name, body in PAGES.items():
        for theme in ("light", "dark"):
            dark = f"<style>{THEME_OVERRIDES[theme]}</style>"
            html = PAGE_TEMPLATE.format(css=css_href, dark=dark, body=body)
            page = PREVIEWS / f"{name}-{theme}.html"
            page.write_text(html)
            if shoot:
                shot = SHOTS / f"{name}-{theme}.png"
                subprocess.run(
                    [
                        CHROME,
                        "--headless",
                        "--disable-gpu",
                        "--hide-scrollbars",
                        f"--screenshot={shot}",
                        "--window-size=1180,840",
                        page.resolve().as_uri(),
                    ],
                    check=True,
                    capture_output=True,
                )
                print(f"shot {shot.relative_to(ROOT)}")


if __name__ == "__main__":
    build(shoot="--html" not in sys.argv)
