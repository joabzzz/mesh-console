import { useEffect, useRef, useState, type ReactNode } from 'react'
import SwitchMeshDialog from './components/SwitchMeshDialog'
import Welcome from './screens/Welcome'
import ComputeDemo, { type ComputeStatus } from './screens/ComputeDemo'
import JoinFlow from './screens/JoinFlow'
import PublicJoin from './screens/PublicJoin'
import PowerSetup from './screens/PowerSetup'
import Visibility from './screens/Visibility'
import Progress from './screens/Progress'
import PublicProgress from './screens/PublicProgress'
import MeshLive from './screens/MeshLive'
import Main from './screens/Main'
import SettingsView from './screens/SettingsView'
import { appApi } from './lib/api'
import { connect, useApp } from './lib/store'
import { clearLastConfig, loadLastConfig, saveLastConfig, type LaunchConfig } from './lib/session'
import type { Visibility as Vis } from './lib/types'

type AppView =
  | { name: 'welcome' }
  | { name: 'join'; prefillToken?: string }
  | { name: 'join-setup'; token: string } // power setup for join-and-share
  | { name: 'public-mode' } // contribute vs passive for the global mesh
  | { name: 'public-setup' } // power setup for a global-mesh contributor
  | { name: 'public-upgrade-setup' } // power setup while already connected passively
  | { name: 'host-setup' }
  | { name: 'host-visibility'; model: string }
  | { name: 'progress'; goal: 'host' | 'join'; flavor?: 'public-passive' | 'public-share' }
  | { name: 'main' }

type View = AppView | { name: 'settings'; from: AppView }

export default function App() {
  const { phase, lastNodeEvent } = useApp()
  const [compute, setCompute] = useState<ComputeStatus | null>(null)
  useEffect(() => {
    void fetch('/app/compute')
      .then((r) => (r.ok ? r.json() : null))
      .then((value) => {
        if (value?.enabled) setCompute(value as ComputeStatus)
      })
      .catch(() => {})
  }, [])
  const [view, setView] = useState<View>({ name: 'welcome' })
  const [booted, setBooted] = useState(false)
  // A model queued to share on the global mesh. A chat-only client node has no
  // AI runtime, so upgrading means shutdown + rejoin with share:true — and
  // shutdown mid-startup races the launch task, so the switch only fires once
  // the phase is 'running' (see the effect below).
  const [pendingShare, setPendingShare] = useState<string | null>(null)
  const [pendingSwitch, setPendingSwitch] = useState<LaunchConfig | null>(null)
  const pendingSwitchRef = useRef<LaunchConfig | null>(null)
  const switchRequestId = useRef(0)
  const [switching, setSwitching] = useState(false)

  const setSwitchRequest = (config: LaunchConfig | null) => {
    pendingSwitchRef.current = config
    switchRequestId.current += 1
    setPendingSwitch(config)
  }

  useEffect(() => {
    connect()
    appApi
      .state()
      .then((p) => {
        if (p.phase === 'running') setView({ name: 'main' })
        else if (p.phase !== 'idle' && p.phase !== 'error')
          setView({ name: 'progress', goal: 'host' })
      })
      .finally(() => setBooted(true))
    // A mesh:// invite link may have launched the app before this frontend was
    // listening — drain the backend's one-shot buffer and open the join flow.
    appApi
      .pendingInvite()
      .then(({ token }) => {
        if (token) setView({ name: 'join', prefillToken: token })
      })
      .catch(() => {
        /* older backend without the endpoint — links still arrive via SSE */
      })
  }, [])

  // Invite links clicked while the app is already running arrive as an
  // invite_link node event over SSE: jump straight into the join flow.
  // setState fires from a timer callback (not the effect body) to avoid
  // cascading renders — react-hooks/set-state-in-effect.
  useEffect(() => {
    if (lastNodeEvent?.event !== 'invite_link') return
    const token = lastNodeEvent.detail.token
    if (typeof token !== 'string' || token.length === 0) return
    const t = setTimeout(() => setView({ name: 'join', prefillToken: token }), 0)
    return () => clearTimeout(t)
  }, [lastNodeEvent])

  useEffect(() => {
    if (phase.phase === 'running' || switching || !pendingSwitchRef.current) return
    const timer = setTimeout(() => setSwitchRequest(null), 0)
    return () => clearTimeout(timer)
  }, [phase.phase, switching])

  // Apply a queued passive→contributor upgrade once the connection is up.
  // No cleanup-cancellation here: the shutdown we issue flips the phase, which
  // would re-run a cancelling effect and abort the rejoin. Instead a ref
  // debounces the fire (StrictMode-safe) and the rejoin double-checks the
  // queue is still set (Cancel clears it mid-flight).
  const pendingShareRef = useRef<string | null>(null)
  useEffect(() => {
    pendingShareRef.current = pendingShare
  }, [pendingShare])
  const upgradeInFlight = useRef(false)
  useEffect(() => {
    if (!pendingShare || phase.phase !== 'running' || upgradeInFlight.current) return
    upgradeInFlight.current = true
    const model = pendingShare
    void (async () => {
      try {
        await appApi.shutdown()
      } catch {
        /* join below still 409s if the node is genuinely stuck */
      }
      if (pendingShareRef.current === model) {
        saveLastConfig({ kind: 'public', share: true, model })
        void appApi.join('', true, model, { public: true })
        setPendingShare(null)
        setView({ name: 'progress', goal: 'join', flavor: 'public-share' })
      }
      upgradeInFlight.current = false
    })()
  }, [pendingShare, phase.phase])

  // Entry point for both upgrade CTAs (public load screen + Main sidebar):
  // queue the model and make sure the public progress screen is what's up
  // while the switch happens.
  const queueShareUpgrade = (model: string) => {
    setPendingShare(model)
    setView({ name: 'progress', goal: 'join', flavor: 'public-passive' })
  }

  if (compute) return <ComputeDemo initial={compute} />

  if (!booted) return null

  // Persist the launch intent, then fire the matching backend call and move to
  // the right progress screen. One place so every front door remembers itself
  // for the next "Back to mesh".
  const launch = (config: LaunchConfig) => {
    saveLastConfig(config)
    switch (config.kind) {
      case 'host':
        void appApi.host(config.model, config.visibility)
        setView({ name: 'progress', goal: 'host' })
        break
      case 'join':
        void appApi.join(config.token, config.share, config.model)
        setView({ name: 'progress', goal: 'join' })
        break
      case 'public':
        void appApi.join('', config.share, config.model, { public: true })
        setView({
          name: 'progress',
          goal: 'join',
          flavor: config.share ? 'public-share' : 'public-passive',
        })
        break
    }
  }

  const requestLaunch = (config: LaunchConfig) => {
    if (phase.phase === 'running') {
      setSwitchRequest(config)
      return
    }
    launch(config)
  }

  const confirmSwitch = async () => {
    if (!pendingSwitchRef.current || switching) return
    const config = pendingSwitchRef.current
    const requestId = switchRequestId.current
    setSwitching(true)
    try {
      await appApi.shutdown()
      if (requestId !== switchRequestId.current || pendingSwitchRef.current !== config) return
      setSwitchRequest(null)
      launch(config)
    } finally {
      setSwitching(false)
    }
  }

  // "Back to mesh" returns to a live connection without relaunching it.
  const resume = () => {
    if (phase.phase === 'running') {
      setView({ name: 'main' })
      return
    }
    const last = loadLastConfig()
    if (last) requestLaunch(last)
  }

  const openSettings = (from: AppView) => setView({ name: 'settings', from })

  const leaveMesh = () => {
    setPendingShare(null)
    clearLastConfig()
    void appApi.shutdown()
    setView({ name: 'welcome' })
  }

  const running = phase.phase === 'running' ? phase : null
  const activeMesh = running
    ? {
        name:
          running.mesh_name ??
          (running.visibility === 'public' ? 'Global mesh' : 'Your private mesh'),
        isPublic: running.visibility === 'public',
      }
    : undefined
  const destinationLabel = pendingSwitch
    ? pendingSwitch.kind === 'public'
      ? 'the Global mesh'
      : pendingSwitch.kind === 'host'
        ? 'a new private mesh'
        : 'the invited private mesh'
    : ''
  const withSwitchDialog = (content: ReactNode) => (
    <>
      {content}
      {pendingSwitch && activeMesh && (
        <SwitchMeshDialog
          currentName={activeMesh.name}
          destination={destinationLabel}
          onCancel={() => setSwitchRequest(null)}
          onConfirm={() => void confirmSwitch()}
          busy={switching}
        />
      )}
    </>
  )

  switch (view.name) {
    case 'welcome':
      return withSwitchDialog(
        <Welcome
          lastConfig={loadLastConfig()}
          onResume={resume}
          onStartFresh={clearLastConfig}
          onJoinPublic={() => setView({ name: 'public-mode' })}
          onJoin={(prefillToken) => setView({ name: 'join', prefillToken })}
          onHost={() => setView({ name: 'host-setup' })}
          onOpenSettings={() => openSettings({ name: 'welcome' })}
          activeMesh={activeMesh}
          onReturnToChat={() => setView({ name: 'main' })}
        />,
      )

    case 'public-mode':
      return withSwitchDialog(
        <PublicJoin
          onBack={() => setView({ name: 'welcome' })}
          onPassive={() => requestLaunch({ kind: 'public', share: false })}
          onContribute={() => setView({ name: 'public-setup' })}
        />,
      )

    case 'public-setup':
      return withSwitchDialog(
        <PowerSetup
          onBack={() => setView({ name: 'public-mode' })}
          onModelChosen={(model) => requestLaunch({ kind: 'public', share: true, model })}
        />,
      )

    case 'public-upgrade-setup':
      // Browse-the-catalog path of the passive→contributor upgrade: the node
      // keeps connecting/running underneath; choosing a model queues the
      // switch, which the pendingShare effect applies once it's safe.
      return (
        <PowerSetup
          onBack={() => setView({ name: 'progress', goal: 'join', flavor: 'public-passive' })}
          onModelChosen={queueShareUpgrade}
        />
      )

    case 'join':
      return withSwitchDialog(
        <JoinFlow
          prefillToken={view.prefillToken}
          onBack={() => setView({ name: 'welcome' })}
          onSubmit={(token, share) => {
            if (share) {
              setView({ name: 'join-setup', token })
            } else {
              requestLaunch({ kind: 'join', token, share: false })
            }
          }}
        />,
      )

    case 'join-setup':
      return withSwitchDialog(
        <PowerSetup
          onBack={() => setView({ name: 'join', prefillToken: view.token })}
          onModelChosen={(model) =>
            requestLaunch({ kind: 'join', token: view.token, share: true, model })
          }
        />,
      )

    case 'host-setup':
      return (
        <PowerSetup
          onBack={() => setView({ name: 'welcome' })}
          onModelChosen={(model) => setView({ name: 'host-visibility', model })}
        />
      )

    case 'host-visibility':
      return withSwitchDialog(
        <Visibility
          onBack={() => setView({ name: 'host-setup' })}
          onChosen={(visibility: Vis) =>
            requestLaunch({ kind: 'host', model: view.model, visibility })
          }
        />,
      )

    case 'progress': {
      // Derived transition: once the backend reaches Running, hosts land on
      // the "mesh is live" QR moment and contributors go straight to the
      // chat. A passive public join instead rests on its own screen ("ready
      // to chat" + the share offer) until the user moves on — and stays there
      // through the reconnect when a share upgrade is queued.
      if (phase.phase === 'running' && view.flavor !== 'public-passive') {
        if (view.goal === 'host') {
          const done = () => setView({ name: 'main' })
          return (
            <MeshLive
              token={phase.invite_token}
              model={phase.model}
              isPrivate={phase.visibility === 'private'}
              onGoToChat={done}
            />
          )
        }
        return (
          <Main
            onLeave={leaveMesh}
            onGoHome={() => setView({ name: 'welcome' })}
            onOpenSettings={() => openSettings({ name: 'main' })}
            onStartSharing={() => setView({ name: 'public-upgrade-setup' })}
          />
        )
      }
      const cancel = () => {
        setPendingShare(null)
        clearLastConfig()
        void appApi.shutdown()
        setView({ name: 'welcome' })
      }
      const errorReset = () => {
        setPendingShare(null)
        clearLastConfig()
        void appApi.reset()
        setView({ name: 'welcome' })
      }
      if (view.flavor) {
        return (
          <PublicProgress
            flavor={view.flavor}
            pendingShare={pendingShare}
            onShareCompute={() => setView({ name: 'public-upgrade-setup' })}
            onStartChatting={() => setView({ name: 'main' })}
            onCancel={cancel}
            onErrorReset={errorReset}
          />
        )
      }
      return <Progress onCancel={cancel} onErrorReset={errorReset} />
    }

    case 'main':
      return (
        <Main
          onLeave={leaveMesh}
          onGoHome={() => setView({ name: 'welcome' })}
          onOpenSettings={() => openSettings({ name: 'main' })}
          onStartSharing={() => setView({ name: 'public-upgrade-setup' })}
        />
      )

    case 'settings':
      return (
        <SettingsView
          onClose={() => setView(view.from)}
          onLeave={phase.phase === 'running' ? leaveMesh : undefined}
        />
      )
  }
}
