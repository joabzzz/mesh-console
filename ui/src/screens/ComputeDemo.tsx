import { useEffect, useState } from 'react'
import { Cpu, LoaderCircle, ShieldCheck, Plus, Power } from 'lucide-react'
import Chat from '../components/Chat'
import MeshPanel from '../components/MeshPanel'
import MeshMark from '../components/MeshMark'
import type { PeerInfo } from '../lib/types'

export interface ComputeStatus {
  enabled: boolean
  stage: 'idle' | 'provisioning' | 'loading' | 'ready' | 'removing' | 'removed' | 'uncertain'
  cap_micro_usd: number
  node: { node_id: string; peers: PeerInfo[] | null; my_hostname?: string } | null
  expires_at: number | null
}
const labels = {
  idle: 'Add a GPU to your demo mesh',
  provisioning: 'Starting your GPU',
  loading: 'Loading Qwen 27B',
  ready: 'Qwen 27B is ready',
  removing: 'Removing compute — waiting for provider confirmation',
  removed: 'GPU removed · provider confirmed',
  uncertain: 'Compute needs attention · cleanup remains active',
}
export default function ComputeDemo({ initial }: { initial: ComputeStatus }) {
  const [status, setStatus] = useState(initial)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [streaming, setStreaming] = useState(false)
  useEffect(() => {
    let stopped = false
    const poll = async () => {
      try {
        const response = await fetch('/app/compute')
        if (!response.ok) throw new Error('Status unavailable. Cleanup remains active.')
        const next = (await response.json()) as ComputeStatus
        if (!stopped) {
          setStatus(next)
          setError('')
        }
      } catch {
        if (!stopped) setError('Status unavailable. Cleanup remains active.')
      }
    }
    const timer = setInterval(() => void poll(), 1500)
    return () => {
      stopped = true
      clearInterval(timer)
    }
  }, [])
  const act = async (verb: 'up' | 'down') => {
    setBusy(true)
    try {
      const response = await fetch(`/app/compute/${verb}`, {
        method: 'POST',
        headers: { 'x-m8s-desktop': '1' },
      })
      if (!response.ok)
        throw new Error('Request could not be confirmed. Check status before continuing.')
      setStatus((current) => ({ ...current, stage: verb === 'up' ? 'provisioning' : 'removing' }))
    } catch {
      setError('Request could not be confirmed. Check status before continuing.')
    } finally {
      setBusy(false)
    }
  }
  const active = ['provisioning', 'loading', 'ready', 'uncertain'].includes(status.stage)
  return (
    <div className="flex h-screen flex-col bg-canvas text-ink">
      <header className="flex items-center justify-between border-b border-edge px-6 py-4">
        <div className="flex items-center gap-3">
          <MeshMark size={26} />
          <span className="font-semibold">Mesh</span>
          <span className="text-sm text-ink-muted">Private GPU demo</span>
        </div>
        <span className="text-sm text-ink-muted">
          Rental cap ${(status.cap_micro_usd / 1_000_000).toFixed(2)} · automatic cleanup
        </span>
      </header>
      <section
        className="flex items-center justify-between gap-4 border-b border-edge bg-panel px-6 py-5"
        aria-live="polite"
      >
        <div className="flex items-center gap-4">
          {status.stage === 'ready' || status.stage === 'removed' ? (
            <ShieldCheck className="text-good" />
          ) : active || status.stage === 'removing' ? (
            <LoaderCircle className="animate-spin text-accent" />
          ) : (
            <Cpu className="text-accent" />
          )}
          <div>
            <h1 className="text-lg font-semibold">{labels[status.stage]}</h1>
            <p className="mt-1 text-sm text-ink-muted">
              {status.node
                ? 'Your Mac is a certified mesh client. Inference runs on the remote GPU.'
                : status.stage === 'removed'
                  ? 'The rental has ended. Your conversation stays visible.'
                  : 'Create a dedicated mesh with a certified local client and one GPU worker.'}
            </p>
          </div>
        </div>
        {status.stage === 'idle' && (
          <button
            data-testid="add-compute"
            className="flex items-center gap-2 rounded-lg bg-accent px-4 py-3 font-semibold text-black disabled:opacity-50"
            disabled={busy || !!error}
            onClick={() => void act('up')}
          >
            <Plus size={18} />
            Add compute · Qwen 27B
          </button>
        )}
        {active && (
          <button
            data-testid="remove-compute"
            className="flex items-center gap-2 rounded-lg border border-edge px-4 py-3 text-sm disabled:opacity-50"
            disabled={busy}
            onClick={() => void act('down')}
          >
            <Power size={16} />
            Remove compute
          </button>
        )}
      </section>
      {error && (
        <p role="alert" className="border-b border-edge px-6 py-2 text-sm text-red-400">
          {error}
        </p>
      )}
      <div className="flex min-h-0 grow">
        <main className="min-w-0 grow">
          <Chat
            sessionId={null}
            pinnedDemo
            ready={status.stage === 'ready' && !error}
            hostname="Qwen 27B GPU"
            onStreamingChange={setStreaming}
          />
        </main>
        {status.node ? (
          <MeshPanel
            meshName="Qwen demo mesh"
            isPrivate
            peers={status.node.peers ?? []}
            self={{ name: 'This Mac · client', serving: false, model: null }}
            streaming={streaming}
            token={null}
            onInvite={() => {}}
          />
        ) : (
          <aside className="w-[320px] border-l border-edge bg-panel p-6 text-sm text-ink-muted">
            <Cpu className="mb-4" />
            {status.stage === 'removed'
              ? 'Provider-confirmed teardown. No rented compute remains from this attempt.'
              : 'Your local node and GPU will appear here as they are observed.'}
          </aside>
        )}
      </div>
    </div>
  )
}
