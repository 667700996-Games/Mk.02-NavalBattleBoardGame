import { browser } from '$app/environment';

export const funnelStages = [
  'landing',
  'tutorial_started',
  'tutorial_completed',
  'session_created',
  'lobby_entered',
  'room_joined',
  'placement_completed',
  'first_attack',
  'match_completed'
] as const;

export type FunnelStage = (typeof funnelStages)[number];
export type FunnelOutcome = 'reached' | 'failed' | 'abandoned';
export type FunnelFailureReason =
  | 'network'
  | 'session_creation'
  | 'authentication'
  | 'room_entry'
  | 'matchmaking'
  | 'recovery'
  | 'placement'
  | 'attack';

interface FunnelEvent {
  stage: FunnelStage;
  outcome: FunnelOutcome;
  reason?: FunnelFailureReason;
}

const endpoint = '/api/telemetry/funnel';
const storagePrefix = 'mk01:funnel:v1:';
const activeStageKey = `${storagePrefix}active-stage`;

function stageIndex(stage: FunnelStage): number {
  return funnelStages.indexOf(stage);
}

function storage(): Storage | null {
  if (!browser) return null;
  try {
    return window.sessionStorage;
  } catch {
    return null;
  }
}

function dispatch(event: FunnelEvent, beacon = false): void {
  if (!browser) return;
  const body = JSON.stringify(event);
  if (beacon && navigator.sendBeacon) {
    const accepted = navigator.sendBeacon(endpoint, new Blob([body], { type: 'application/json' }));
    if (accepted) return;
  }
  void fetch(endpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body,
    keepalive: beacon
  }).catch(() => {
    // Funnel reporting must never block or break the player flow.
  });
}

export function trackFunnelReached(stage: FunnelStage): void {
  const store = storage();
  const dedupeKey = `${storagePrefix}reached:${stage}`;
  if (store?.getItem(dedupeKey)) return;
  store?.setItem(dedupeKey, '1');
  dispatch({ stage, outcome: 'reached' });

  if (stage === 'match_completed') {
    store?.removeItem(activeStageKey);
    return;
  }
  const current = store?.getItem(activeStageKey) as FunnelStage | null;
  if (!current || stageIndex(stage) >= stageIndex(current)) store?.setItem(activeStageKey, stage);
}

export function trackFunnelFailure(stage: FunnelStage, reason: FunnelFailureReason): void {
  dispatch({ stage, outcome: 'failed', reason });
}

export function trackFunnelAbandoned(stage?: FunnelStage): void {
  const store = storage();
  const active = stage ?? (store?.getItem(activeStageKey) as FunnelStage | null);
  if (!active || active === 'match_completed') return;
  const dedupeKey = `${storagePrefix}abandoned:${active}`;
  if (store?.getItem(dedupeKey)) return;
  store?.setItem(dedupeKey, '1');
  dispatch({ stage: active, outcome: 'abandoned' }, true);
}

export function installFunnelAbandonmentTracking(): () => void {
  if (!browser) return () => undefined;
  const onPageHide = () => trackFunnelAbandoned();
  window.addEventListener('pagehide', onPageHide);
  return () => window.removeEventListener('pagehide', onPageHide);
}
