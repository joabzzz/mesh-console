import {
  ArrowUp,
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  FileText,
  Gauge,
  Globe,
  Lightbulb,
  Mail,
  Radio,
  Square,
  Terminal,
  Utensils,
  Wrench,
  X,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { appApi, streamChat } from '../lib/api'
import type { ChatMessage, ChatToolCall } from '../lib/types'
import MeshMark from './MeshMark'

const STARTERS = [
  { icon: Utensils, text: 'Plan a week of dinners' },
  { icon: Lightbulb, text: "Explain something to me like I'm five" },
  { icon: Mail, text: 'Help me write a tricky email' },
]

interface ChatProps {
  sessionId: string | null
  pinnedDemo?: boolean
  ready?: boolean
  hostname?: string
  onStreamingChange?: (streaming: boolean) => void
  onConversationChanged?: () => void
}

/** Split inline `<think>…</think>` blocks (small reasoning models emit these
 *  in output_text) away from the visible answer. */
function splitThinking(raw: string): { thinking: string; answer: string } {
  if (!raw.startsWith('<think>')) return { thinking: '', answer: raw }
  const end = raw.indexOf('</think>')
  if (end === -1) return { thinking: raw.slice(7), answer: '' }
  return { thinking: raw.slice(7, end).trim(), answer: raw.slice(end + 8).replace(/^\s+/, '') }
}

export default function Chat({
  sessionId,
  pinnedDemo = false,
  ready = true,
  hostname,
  onStreamingChange,
  onConversationChanged,
}: ChatProps) {
  const selectedModel = 'auto'
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [input, setInput] = useState('')
  const [streaming, setStreaming] = useState(false)
  const abortRef = useRef<AbortController | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const rawRef = useRef('')
  // Reasoning streamed as separate reasoning deltas (vs inline <think> tags).
  const thinkingRef = useRef('')
  const idSeq = useRef(0)

  useEffect(() => {
    onStreamingChange?.(streaming)
  }, [streaming, onStreamingChange])

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight })
  }, [messages])

  // Goose remains the canonical local transcript store. Session-aware
  // backends address a specific chat; older backends retain the legacy route.
  useEffect(() => {
    let cancelled = false
    const request = sessionId ? appApi.sessionHistory(sessionId) : appApi.history()
    request
      .then((past) => {
        if (cancelled) return
        setMessages(
          past.map((m) => ({
            id: m.id,
            role: m.role,
            text: m.text,
            thinking: m.thinking,
            toolCalls: m.tool_calls,
          })),
        )
      })
      .catch(() => {
        if (!cancelled) setMessages([])
      })
    return () => {
      cancelled = true
    }
  }, [sessionId])

  const send = async (text: string) => {
    const prompt = text.trim()
    if (!prompt || streaming || !ready) return
    setInput('')
    const userMsg: ChatMessage = { id: `u-${++idSeq.current}`, role: 'user', text: prompt }
    const asstId = `a-${++idSeq.current}`
    setMessages([
      ...messages,
      userMsg,
      { id: asstId, role: 'assistant', text: '', streaming: true },
    ])
    setStreaming(true)
    rawRef.current = ''
    thinkingRef.current = ''

    const update = (patch: Partial<ChatMessage>) =>
      setMessages((msgs) => msgs.map((m) => (m.id === asstId ? { ...m, ...patch } : m)))
    const updateTool = (id: string, patch: Partial<ChatToolCall>) =>
      setMessages((msgs) =>
        msgs.map((m) =>
          m.id === asstId
            ? { ...m, toolCalls: m.toolCalls?.map((t) => (t.id === id ? { ...t, ...patch } : t)) }
            : m,
        ),
      )

    const abort = new AbortController()
    abortRef.current = abort
    try {
      // The agent owns the conversation history — send only the new message.
      await streamChat(
        selectedModel,
        prompt,
        sessionId,
        {
          onDelta: (delta) => {
            rawRef.current += delta
            const { thinking, answer } = splitThinking(rawRef.current)
            update({ text: answer, thinking: thinkingRef.current + thinking })
          },
          onReasoningDelta: (delta) => {
            thinkingRef.current += delta
            update({
              thinking:
                thinkingRef.current +
                (rawRef.current ? splitThinking(rawRef.current).thinking : ''),
            })
          },
          onToolCall: (tool) =>
            setMessages((msgs) =>
              msgs.map((m) =>
                m.id === asstId
                  ? {
                      ...m,
                      toolCalls: [...(m.toolCalls ?? []), { ...tool, status: 'running' as const }],
                    }
                  : m,
              ),
            ),
          onToolResult: ({ id, ok }) => updateTool(id, { status: ok ? 'done' : 'failed' }),
          onCompleted: (info) => update({ completed: info, streaming: false }),
          onError: (message) => update({ error: message, streaming: false }),
        },
        abort.signal,
      )
    } catch (err) {
      if (!abort.signal.aborted) update({ error: String(err), streaming: false })
    } finally {
      update({ streaming: false })
      setStreaming(false)
      abortRef.current = null
      onConversationChanged?.()
    }
  }

  const stop = () => abortRef.current?.abort()

  return (
    <div className="flex h-full min-w-0 flex-col">
      {/* Routing is intentionally automatic. The mesh chooses a model for
          every turn; there is no client-side pinning dropdown. */}
      <div className="flex items-center gap-2 border-b border-edge px-5 py-3">
        <Brain size={14} className="text-accent" aria-hidden />
        <span className="text-[13px] font-medium">{pinnedDemo ? 'Qwen 27B' : 'Auto routing'}</span>
        <span className="text-[11px] text-ink-faint">
          {pinnedDemo
            ? ready
              ? 'Verified remote GPU · conversation through your mesh'
              : 'Waiting for verified compute'
            : 'The mesh balances latency and capability'}
        </span>
        <span className="ml-auto flex items-center gap-1.5 font-mono text-[10px] text-ink-faint">
          <Radio size={10} aria-hidden /> {pinnedDemo ? 'pinned' : 'auto'}
        </span>
      </div>

      {/* messages */}
      <div ref={scrollRef} className="grow overflow-y-auto px-6 py-8" data-testid="chat-messages">
        {messages.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-5 text-center">
            <MeshMark size={44} className="text-accent opacity-90" pulse />
            <h2 className="font-display text-[26px] font-bold tracking-tight">Say hello.</h2>
            <p className="max-w-sm text-sm leading-relaxed text-ink-muted">
              {pinnedDemo
                ? 'Ask Qwen 27B a question, then follow up. Your Mac runs no local model.'
                : 'Your message stays on your mesh. Auto routing chooses the best available host for the turn, balancing responsiveness and capability.'}
            </p>
            <div className="mt-2 flex flex-wrap justify-center gap-2.5">
              {(pinnedDemo
                ? [{ icon: Lightbulb, text: 'Explain a mesh network in two sentences.' }]
                : STARTERS
              ).map((s) => (
                <button
                  key={s.text}
                  disabled={!ready}
                  onClick={() => void send(s.text)}
                  className="flex items-center gap-2 rounded-full border border-edge bg-panel px-4 py-2 text-[13px] text-ink-muted transition-all hover:-translate-y-0.5 hover:border-accent/60 hover:text-ink"
                >
                  <s.icon size={14} className="text-accent" aria-hidden />
                  {s.text}
                </button>
              ))}
            </div>
          </div>
        ) : (
          <div className="mx-auto flex max-w-3xl flex-col gap-6">
            {messages.map((m) =>
              m.role === 'user' ? (
                <div
                  key={m.id}
                  data-testid="user-message"
                  className="animate-message-in max-w-[85%] self-end rounded-(--radius-card) rounded-br-md bg-inset px-4 py-3 text-[15px] leading-relaxed"
                >
                  {m.text}
                </div>
              ) : (
                <AssistantBubble
                  key={m.id}
                  msg={m}
                  modelLabel={pinnedDemo ? 'Qwen 27B' : 'Auto'}
                  hostname={hostname}
                />
              ),
            )}
          </div>
        )}
      </div>

      {/* composer */}
      <div className="border-t border-edge px-6 py-4">
        <div className="mx-auto flex max-w-3xl items-end gap-2">
          <textarea
            data-testid="chat-input"
            value={input}
            rows={1}
            placeholder="Ask anything…"
            aria-label="Prompt"
            disabled={!ready}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault()
                void send(input)
              }
              if (e.key === 'Escape' && streaming) stop()
            }}
            className="max-h-40 grow resize-none rounded-(--radius-card) border border-edge bg-inset px-4 py-3 text-[15px] outline-none transition-shadow focus:border-accent focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--color-accent)_18%,transparent)]"
          />
          {streaming ? (
            <button
              data-testid="chat-stop"
              onClick={stop}
              className="flex items-center gap-2 rounded-(--radius-control) border border-edge bg-panel px-4 py-3 text-sm transition-colors hover:border-bad/60 hover:text-bad"
              aria-label="Stop"
            >
              <Square size={13} fill="currentColor" aria-hidden /> Stop
            </button>
          ) : (
            <button
              data-testid="chat-send"
              onClick={() => void send(input)}
              disabled={!input.trim() || !ready}
              className="rounded-(--radius-control) bg-accent p-3 text-accent-ink transition-all hover:bg-accent-hover active:scale-95 disabled:opacity-40"
              aria-label="Send"
            >
              <ArrowUp size={18} strokeWidth={2.5} aria-hidden />
            </button>
          )}
        </div>
        <p className="mt-2.5 text-center text-[12px] text-ink-faint">
          Private to your mesh · end-to-end encrypted
        </p>
      </div>
    </div>
  )
}

/** goose namespaces tool names as `extension__tool`; show just the tool. */
function toolDisplayName(name: string): string {
  const idx = name.lastIndexOf('__')
  return idx === -1 ? name : name.slice(idx + 2)
}

/** Pick an icon that says what kind of work the tool is doing. */
function toolIcon(name: string): typeof Wrench {
  const tool = toolDisplayName(name).toLowerCase()
  if (/shell|bash|terminal|command|process/.test(tool)) return Terminal
  if (/web|scrape|fetch|http|search|browse/.test(tool)) return Globe
  if (/file|read|write|edit|text|document|pdf/.test(tool)) return FileText
  return Wrench
}

function ToolStatusIcon({ status }: { status: ChatToolCall['status'] }) {
  if (status === 'running')
    return (
      <span
        className="h-3 w-3 animate-spin rounded-full border-[1.5px] border-inset border-t-accent"
        aria-hidden
      />
    )
  if (status === 'failed') return <X size={12} className="text-bad" aria-hidden />
  return <Check size={12} className="text-good" aria-hidden />
}

/** The last non-empty line of the chain of thought — the live "what is it
 *  doing right now" tail shown while reasoning streams. */
function lastThought(thinking: string): string {
  const lines = thinking.split('\n').filter((l) => l.trim().length > 0)
  return lines[lines.length - 1] ?? ''
}

/**
 * The agent's working trace: chain of thought + tool activity, presented as a
 * single collapsible timeline above the answer. While the turn streams, the
 * header shimmers and the latest thought shows as a live one-line tail; when
 * done it folds to a quiet "Thought process" affordance.
 */
function ActivityTrace({ msg }: { msg: ChatMessage }) {
  const [open, setOpen] = useState(false)
  const hasThinking = Boolean(msg.thinking)
  const tools = msg.toolCalls ?? []
  if (!hasThinking && tools.length === 0) return null

  const liveTail = msg.streaming && hasThinking && !open ? lastThought(msg.thinking ?? '') : null

  return (
    <div className="mb-3 overflow-hidden rounded-(--radius-control) border border-edge/70 bg-inset/40">
      {hasThinking ? (
        <button
          onClick={() => setOpen((s) => !s)}
          className="flex w-full items-center gap-2 px-3 py-2 text-left text-[12px] text-ink-faint transition-colors hover:text-ink-muted"
          data-testid="thinking-toggle"
          aria-expanded={open}
        >
          {open ? <ChevronDown size={13} aria-hidden /> : <ChevronRight size={13} aria-hidden />}
          <Brain size={13} aria-hidden />
          <span className={msg.streaming ? 'text-shimmer' : ''}>
            {msg.streaming ? 'Thinking…' : 'Thought process'}
          </span>
        </button>
      ) : (
        <div className="flex items-center gap-2 px-3 py-2 text-[12px] text-ink-faint">
          <Wrench size={13} aria-hidden />
          <span className={msg.streaming ? 'text-shimmer' : ''}>
            {msg.streaming ? 'Working…' : 'Tool activity'}
          </span>
        </div>
      )}

      {liveTail && (
        <p
          className="truncate px-3 pb-2 pl-9 font-mono text-[11.5px] text-ink-faint italic"
          data-testid="thinking-live-tail"
        >
          {liveTail}
        </p>
      )}

      {hasThinking && open && (
        <div className="border-t border-edge/60 px-3 py-2.5 pl-9">
          <p className="text-[13px] leading-relaxed whitespace-pre-wrap text-ink-faint">
            {msg.thinking}
          </p>
        </div>
      )}

      {tools.length > 0 && (
        <div className="flex flex-col gap-1 border-t border-edge/60 px-3 py-2 pl-9">
          {tools.map((t) => {
            const Icon = toolIcon(t.name)
            return (
              <div
                key={t.id}
                data-testid="tool-chip"
                data-status={t.status}
                className={`flex items-center gap-2 font-mono text-[11.5px] ${
                  t.status === 'running'
                    ? 'text-ink-muted'
                    : t.status === 'failed'
                      ? 'text-bad'
                      : 'text-ink-faint'
                }`}
                title={t.name}
              >
                <ToolStatusIcon status={t.status} />
                <Icon size={12} aria-hidden />
                {toolDisplayName(t.name)}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

function AssistantBubble({
  msg,
  modelLabel,
  hostname,
}: {
  msg: ChatMessage
  modelLabel?: string
  hostname?: string
}) {
  // Nothing visible yet and no tool mid-flight: the model is warming up or
  // prefilling (can be minutes for a cold big model — KV cache allocation).
  // Show shimmering feedback instead of a dead bubble.
  const waiting =
    Boolean(msg.streaming) &&
    !msg.text &&
    !msg.thinking &&
    !msg.error &&
    !msg.toolCalls?.some((t) => t.status === 'running')
  const tokPerSec = (() => {
    const out = msg.completed?.usage?.output_tokens
    const ms = msg.completed?.timings?.decode_time_ms
    if (!out || !ms) return null
    return Math.round((out / ms) * 1000)
  })()
  const servedBy = msg.completed?.served_by

  return (
    <div className="animate-message-in flex gap-3 self-start" data-testid="assistant-message">
      <div className="mt-1 shrink-0 text-accent" aria-hidden>
        <MeshMark size={22} pulse={Boolean(msg.streaming)} />
      </div>
      <div className="min-w-0 grow">
        <div className="mb-1.5 flex items-center gap-2 font-mono text-[11px] text-ink-faint">
          <span>{modelLabel ?? 'model'}</span>
          {servedBy && (
            <span className="flex items-center gap-1">
              <Radio size={10} aria-hidden />
              via {servedBy === hostname ? 'this Mac' : servedBy}
            </span>
          )}
        </div>

        <ActivityTrace msg={msg} />

        {waiting && (
          <p className="text-shimmer text-[14px]" data-testid="assistant-waiting">
            thinking…
          </p>
        )}
        <div className="prose-mesh text-[15px] leading-relaxed" data-testid="assistant-text">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{msg.text}</ReactMarkdown>
          {msg.streaming && msg.text && <span className="stream-caret" aria-hidden />}
        </div>
        {msg.error && (
          <p className="mt-2 flex items-center gap-1.5 text-[13px] text-bad">
            <X size={13} aria-hidden />
            {msg.error}
          </p>
        )}
        {tokPerSec !== null && (
          <div
            className="mt-2 flex items-center gap-1.5 font-mono text-[11px] text-ink-faint"
            title={`${tokPerSec} tokens per second — how fast the model writes`}
          >
            <Gauge size={11} aria-hidden />
            {tokPerSec} tok/s
          </div>
        )}
      </div>
    </div>
  )
}
