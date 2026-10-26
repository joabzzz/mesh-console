import type { Page } from '@playwright/test'

/**
 * Installs a self-contained mock of the app API + node API inside the page,
 * before any app code runs — the pattern proven by mesh-llm-ui's
 * live-parity.spec.ts (window.fetch shim + MockEventSource). Timings are
 * compressed so full onboarding flows run in ~1s.
 */
export interface MockOptions {
  /** Boot the mock already in the running state (skips onboarding). */
  startRunning?: boolean
  /** Overrides merged into the running phase when startRunning is set — e.g.
   *  boot as a public chat-only join instead of the default private host. */
  runningPhase?: Record<string, unknown>
  /** Make /app/diagnose report a model already downloaded, so PowerSetup takes
   *  the "you've already got models" fast path instead of the scan beat. */
  installedInDiagnose?: boolean
  /** Seed GET /app/pending_invite (one-shot), simulating a mesh:// invite
   *  link that launched the app before the frontend was listening. */
  pendingInvite?: string
  peers?: Array<Record<string, unknown>>
  /** Seed GET /app/history so the chat repaints a prior conversation on mount
   *  (simulates a persistent goose session surviving an app restart). */
  history?: Array<{
    id: string
    role: 'user' | 'assistant'
    text: string
    thinking?: string
    tool_calls?: Array<{ id: string; name: string; status: 'done' | 'failed' }>
  }>
}

export async function installMockBackend(page: Page, options: MockOptions = {}) {
  await page.addInitScript((opts: MockOptions) => {
    const TOKEN =
      'eyJpZCI6Im1vY2stbm9kZS1pZC0wMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAiLCJhZGRycyI6W3siUmVsYXkiOiJodHRwczovL21vY2sucmVsYXkuaXJvaC5saW5rLyJ9LHsiSXAiOiIxOTIuMTY4LjEuMjA6NTAwMDAifV19'

    const DIAGNOSE = {
      hardware: {
        gpu_name: 'Apple M5 Max',
        gpu_count: 1,
        is_soc: true,
        vram_bytes: 115448725504,
        vram_gb: 115.4,
        vram_display: '128 GB',
        hostname: 'test-mac.local',
        disk_free_bytes: 812000000000,
        disk_free_display: '812 GB',
      },
      recommended: {
        name: 'Qwen3-Coder-Next-Q4_K_M',
        reason: 'Best fit for 128 GB of AI memory',
      },
      catalog: [
        {
          name: 'Qwen3-Coder-Next-Q4_K_M',
          file: 'q.gguf',
          size: '48GB',
          size_gb: 48,
          description: 'Frontier coding model',
          fit: 'comfortable',
          installed: false,
          recommended: true,
          draft: false,
        },
        {
          name: 'GLM-4.7-Flash-Q4_K_M',
          file: 'g.gguf',
          size: '18GB',
          size_gb: 18,
          description: 'Fast, capable all-rounder for everyday use.',
          fit: 'comfortable',
          installed: false,
          recommended: false,
          draft: false,
        },
        {
          name: 'Qwen3-0.6B-Q4_K_M',
          file: 't.gguf',
          size: '397MB',
          size_gb: 0.397,
          description: 'Small and quick.',
          fit: 'comfortable',
          installed: false,
          recommended: false,
          draft: false,
        },
        {
          name: 'MiniMax-M2.5-Q4_K_M',
          file: 'm.gguf',
          size: '138GB',
          size_gb: 138,
          description: 'Flagship.',
          fit: 'too_large',
          installed: false,
          recommended: false,
          draft: false,
        },
      ],
    }

    const MODEL_ID = 'unsloth/Qwen3-0.6B-GGUF:Q4_K_M'
    const STATUS = {
      node_id: 'mock-node',
      node_state: 'serving',
      llama_ready: true,
      hostname: 'test-mac.local',
      peers: opts.peers ?? [],
      models: [MODEL_ID],
      serving_models: ['unsloth/Qwen3-0.6B-GGUF@main:Q4_K_M'],
      my_vram_gb: 115.4,
      token: TOKEN,
      publication_state: 'private',
    }

    type Json = Record<string, unknown>
    const RUNNING_PHASE: Json = {
      phase: 'running',
      mode: 'host',
      visibility: 'private',
      model: 'Qwen3-0.6B-Q4_K_M',
      serving: true,
      invite_token: TOKEN,
      api_port: 9337,
      console_port: 3131,
      mesh_name: null,
    }
    const state = {
      phase: (opts.startRunning
        ? { ...RUNNING_PHASE, ...(opts.runningPhase ?? {}) }
        : { phase: 'idle' }) as Json,
      hostCalls: [] as Json[],
      joinCalls: [] as Json[],
      chatCalls: [] as Json[],
      newChatCalls: [] as Json[],
      // Goose-backed conversations exposed through the desktop session API.
      histories: { 'mock-session-1': (opts.history ?? []) as Json[] } as Record<string, Json[]>,
      sessions: [
        {
          id: 'mock-session-1',
          name: opts.history?.[0]?.text ?? 'New chat',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          message_count: opts.history?.length ?? 0,
          active: true,
          archived: false,
        },
      ] as Json[],
      activeSessionId: 'mock-session-1',
      shutdownCalls: [] as Json[],
      // One-shot: cleared on first read, like the real backend.
      pendingInvite: (opts.pendingInvite ?? null) as string | null,
    }

    // ---- EventSource mock ----
    const instances: Array<{ url: string; es: MockES }> = []
    class MockES {
      url: string
      onmessage: ((e: { data: string }) => void) | null = null
      onopen: (() => void) | null = null
      onerror: (() => void) | null = null
      constructor(url: string) {
        this.url = String(url)
        instances.push({ url: this.url, es: this })
        setTimeout(() => {
          this.onopen?.()
          if (this.url.includes('/app/events')) {
            this.emit({ type: 'phase', ...state.phase })
          }
          if (this.url.includes('/api/events')) {
            this.emit(STATUS)
          }
        }, 10)
      }
      emit(obj: unknown) {
        this.onmessage?.({ data: JSON.stringify(obj) })
      }
      close() {}
      addEventListener() {}
    }
    ;(window as unknown as Json).EventSource = MockES

    const emitApp = (obj: Json) => {
      if (obj.type === 'phase') {
        const { type: _t, ...phase } = obj
        state.phase = phase
      }
      instances.filter((i) => i.url.includes('/app/events')).forEach((i) => i.es.emit(obj))
    }
    ;(window as unknown as Json).__emitApp = emitApp
    ;(window as unknown as Json).__mockState = state

    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

    const runLaunchSequence = async (target: Json, download: boolean) => {
      // Chat-only joins skip the runtime/model beats entirely, mirroring the
      // real backend (node.rs run_join only installs when share=true).
      if (download) {
        emitApp({ type: 'phase', phase: 'installing_runtime' })
        await sleep(120)
        emitApp({ type: 'phase', phase: 'downloading', model: target.model ?? 'model' })
        // Long enough (~2s) for the UI's windowed rate/ETA to appear and be
        // asserted before the phase moves on.
        for (const pct of [5, 15, 28, 42, 55, 68, 82, 94]) {
          await sleep(250)
          emitApp({
            type: 'download_progress',
            kind: 'model',
            label: String(target.model ?? 'model'),
            file: 'model-file.gguf',
            downloaded_bytes: pct * 4e6,
            total_bytes: 400e6,
            status: 'downloading',
            done: false,
          })
        }
      }
      await sleep(120)
      emitApp({ type: 'phase', phase: 'starting', mode: target.mode, model: target.model ?? null })
      await sleep(150)
      emitApp({
        type: 'phase',
        phase: 'running',
        mode: target.mode,
        visibility: target.visibility ?? 'private',
        model: target.model ?? null,
        serving: target.serving,
        invite_token: TOKEN,
        api_port: 9337,
        console_port: 3131,
        mesh_name: target.mesh_name ?? null,
      })
    }

    const sseResponse = (frames: string[]) => {
      const stream = new ReadableStream({
        async start(controller) {
          const enc = new TextEncoder()
          // Mimic real TTFT: a beat of silence before the first frame, so the
          // UI's "thinking…" waiting shimmer has an assertable window.
          await sleep(300)
          for (const frame of frames) {
            controller.enqueue(enc.encode(frame))
            await sleep(40)
          }
          controller.close()
        },
      })
      return new Response(stream, {
        status: 200,
        headers: { 'content-type': 'text/event-stream' },
      })
    }

    // Mirrors the real /app/chat contract: an agent turn with a tool call,
    // <think>-tagged deltas, and a completed frame WITHOUT served_by/timings
    // (the client computes timing fallbacks itself).
    const chatFrames = () => {
      const frame = (event: string, payload: Json) =>
        `event: ${event}\ndata: ${JSON.stringify({ type: event, ...payload })}\n\n`
      const frames = [
        frame('response.tool_call', {
          id: 't1',
          name: 'computercontroller__web_scrape',
        }),
      ]
      const deltas = [
        '<think>',
        'pondering the mesh',
        '</think>',
        'Hello',
        ' from',
        ' the',
        ' mesh',
        '.',
      ]
      frames.push(...deltas.map((d) => frame('response.output_text.delta', { delta: d })))
      frames.push(
        frame('response.tool_result', { id: 't1', ok: true }),
        frame('response.completed', {
          response: {
            model: MODEL_ID,
            usage: { input_tokens: 12, output_tokens: 40 },
          },
        }),
        'data: [DONE]\n\n',
      )
      return frames
    }

    const json = (obj: unknown, status = 200) =>
      new Response(JSON.stringify(obj), {
        status,
        headers: { 'content-type': 'application/json' },
      })

    const realFetch = window.fetch.bind(window)
    window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url
      const path = new URL(url, location.origin).pathname

      if (path === '/app/compute') return json({ enabled: false })
      if (path === '/app/state') return json(state.phase)
      if (path === '/app/diagnose') {
        if (opts.installedInDiagnose) {
          return json({
            ...DIAGNOSE,
            catalog: (DIAGNOSE.catalog as Json[]).map((m) =>
              m.name === 'Qwen3-0.6B-Q4_K_M' ? { ...m, installed: true } : m,
            ),
          })
        }
        return json(DIAGNOSE)
      }
      if (path === '/app/host') {
        const body = JSON.parse(String(init?.body ?? '{}'))
        state.hostCalls.push(body)
        void runLaunchSequence(
          { mode: 'host', model: body.model, visibility: body.visibility, serving: true },
          true,
        )
        return json({ ok: true }, 202)
      }
      if (path === '/app/join') {
        const body = JSON.parse(String(init?.body ?? '{}'))
        state.joinCalls.push(body)
        void runLaunchSequence(
          {
            mode: 'join',
            model: body.model,
            visibility: body.public ? 'public' : 'private',
            serving: Boolean(body.share),
            mesh_name: body.public ? 'Global mesh' : null,
          },
          Boolean(body.share),
        )
        return json({ ok: true }, 202)
      }
      if (path === '/app/invite') return json({ token: TOKEN, approx_bytes: TOKEN.length })
      if (path === '/app/pending_invite') {
        const token = state.pendingInvite
        state.pendingInvite = null
        return json({ token })
      }
      if (path === '/app/shutdown' || path === '/app/reset') {
        if (path === '/app/shutdown') state.shutdownCalls.push({})
        // Emit (not just mutate) so the store sees the running→idle hop the
        // passive→contributor upgrade relies on.
        emitApp({ type: 'phase', phase: 'idle' })
        return json({ ok: true })
      }
      if (path === '/api/status') return json(STATUS)
      if (path === '/api/models') return json({ mesh_models: [] })
      if (path === '/v1/models')
        return json({ data: [{ id: MODEL_ID, display_name: 'Qwen3-0.6B-Q4_K_M' }] })
      if (path === '/app/sessions' && (!init?.method || init.method === 'GET')) {
        return json(state.sessions)
      }
      if (path === '/app/sessions' && init?.method === 'POST') {
        const id = `mock-session-${state.sessions.length + 1}`
        const session = {
          id,
          name: 'New chat',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          message_count: 0,
          active: true,
        }
        state.sessions.forEach((item) => (item.active = false))
        state.sessions.unshift(session)
        state.activeSessionId = id
        state.histories[id] = []
        return json(session, 201)
      }
      const sessionMatch = path.match(/^\/app\/sessions\/([^/]+)\/(activate|history)$/)
      if (sessionMatch?.[2] === 'activate') {
        state.activeSessionId = decodeURIComponent(sessionMatch[1])
        state.sessions.forEach((item) => (item.active = item.id === state.activeSessionId))
        return json({ ok: true })
      }
      if (sessionMatch?.[2] === 'history') {
        const id = decodeURIComponent(sessionMatch[1])
        return json(state.histories[id] ?? [])
      }
      const archiveMatch = path.match(/^\/app\/sessions\/([^/]+)\/archive$/)
      if (archiveMatch) {
        const body = JSON.parse(String(init?.body ?? '{}'))
        const id = decodeURIComponent(archiveMatch[1])
        const session = state.sessions.find((item) => item.id === id)
        if (session) session.archived = body.archived !== false
        return json({ ok: true })
      }
      if (path === '/app/history') return json(state.histories[state.activeSessionId] ?? [])
      if (path === '/app/new_chat') {
        state.newChatCalls.push({})
        state.histories[state.activeSessionId] = []
        return json({ ok: true })
      }
      if (path === '/app/chat') {
        state.chatCalls.push(JSON.parse(String(init?.body ?? '{}')))
        return sseResponse(chatFrames())
      }

      return realFetch(input as RequestInfo, init)
    }
  }, options)
}
