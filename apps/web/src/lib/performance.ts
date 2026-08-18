import { browser } from '$app/environment';

export type RumMetric = 'lcp' | 'cls' | 'inp' | 'battle_interaction';
export type RumRoute =
  'landing' | 'tutorial' | 'lobby' | 'join' | 'room' | 'account' | 'replay' | 'other';
export type RumDeviceTier = 'desktop' | 'mobile' | 'low_mobile';

interface RumSample {
  metric: RumMetric;
  route: RumRoute;
  deviceTier: RumDeviceTier;
  value: number;
}

interface LayoutShiftEntry extends PerformanceEntry {
  value: number;
  hadRecentInput: boolean;
}

interface EventTimingEntry extends PerformanceEntry {
  duration: number;
  interactionId: number;
}

const endpoint = '/api/telemetry/performance';
const battleStarts = new Map<string, number>();

export function classifyRumRoute(pathname: string): RumRoute {
  if (pathname === '/') return 'landing';
  if (pathname.startsWith('/tutorial')) return 'tutorial';
  if (pathname.startsWith('/lobby')) return 'lobby';
  if (pathname.startsWith('/join/')) return 'join';
  if (pathname.startsWith('/room/')) return 'room';
  if (pathname.startsWith('/settings') || pathname.startsWith('/stats')) return 'account';
  if (pathname.startsWith('/replay/')) return 'replay';
  return 'other';
}

export function detectRumDeviceTier(): RumDeviceTier {
  if (!browser) return 'desktop';
  const memory = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
  const cores = navigator.hardwareConcurrency;
  const coarse = matchMedia('(pointer: coarse)').matches;
  if (innerWidth <= 390 || (coarse && ((memory ?? 8) <= 4 || cores <= 4))) return 'low_mobile';
  if (innerWidth <= 768 || coarse) return 'mobile';
  return 'desktop';
}

function dispatch(sample: RumSample, beacon = false): void {
  if (!browser) return;
  const body = JSON.stringify({ ...sample, value: Math.max(0, Math.round(sample.value)) });
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
    // Aggregate RUM reporting must never block or break gameplay.
  });
}

export function reportRumMetric(metric: RumMetric, value: number, beacon = false): void {
  if (!browser || !Number.isFinite(value) || value < 0) return;
  dispatch(
    {
      metric,
      route: classifyRumRoute(location.pathname),
      deviceTier: detectRumDeviceTier(),
      value
    },
    beacon
  );
}

export function trackBattleInteractionStarted(requestId: string): void {
  if (!browser) return;
  const startedAt = performance.now();
  battleStarts.set(requestId, startedAt);
  setTimeout(() => {
    if (battleStarts.get(requestId) === startedAt) battleStarts.delete(requestId);
  }, 60_000);
}

export function cancelBattleInteraction(requestId: string): void {
  battleStarts.delete(requestId);
}

export function trackBattleInteractionResult(requestId: string): void {
  if (!browser) return;
  const startedAt = battleStarts.get(requestId);
  if (startedAt === undefined) return;
  battleStarts.delete(requestId);
  reportRumMetric('battle_interaction', performance.now() - startedAt);
}

export function installRealUserMonitoring(): () => void {
  if (!browser || !('PerformanceObserver' in window)) return () => undefined;
  const route = classifyRumRoute(location.pathname);
  const deviceTier = detectRumDeviceTier();
  const observers: PerformanceObserver[] = [];
  const drainObservers: Array<() => void> = [];
  let lcp = 0;
  let maxCls = 0;
  let clsWindow = 0;
  let clsWindowStartedAt = 0;
  let lastClsAt = 0;
  let hasClsWindow = false;
  const interactions = new Map<number, number>();
  let flushed = false;

  const observe = (
    type: string,
    callback: (entries: PerformanceEntry[]) => void,
    options: PerformanceObserverInit = { type, buffered: true }
  ) => {
    if (!PerformanceObserver.supportedEntryTypes.includes(type)) return;
    try {
      const observer = new PerformanceObserver((list) => callback(list.getEntries()));
      observer.observe(options);
      observers.push(observer);
      drainObservers.push(() => callback(observer.takeRecords()));
    } catch {
      // Unsupported entry options are expected on older browsers.
    }
  };

  observe('largest-contentful-paint', (entries) => {
    const latest = entries.at(-1);
    if (latest) lcp = Math.max(lcp, latest.startTime);
  });
  observe('layout-shift', (entries) => {
    for (const entry of entries as LayoutShiftEntry[]) {
      if (entry.hadRecentInput) continue;
      if (
        !hasClsWindow ||
        entry.startTime - lastClsAt > 1_000 ||
        entry.startTime - clsWindowStartedAt > 5_000
      ) {
        clsWindow = entry.value;
        clsWindowStartedAt = entry.startTime;
        hasClsWindow = true;
      } else {
        clsWindow += entry.value;
      }
      lastClsAt = entry.startTime;
      maxCls = Math.max(maxCls, clsWindow);
    }
  });
  observe(
    'event',
    (entries) => {
      for (const entry of entries as EventTimingEntry[]) {
        if (!entry.interactionId) continue;
        interactions.set(
          entry.interactionId,
          Math.max(interactions.get(entry.interactionId) ?? 0, entry.duration)
        );
      }
    },
    { type: 'event', buffered: true, durationThreshold: 16 } as PerformanceObserverInit
  );

  const flush = () => {
    if (flushed) return;
    flushed = true;
    drainObservers.forEach((drain) => drain());
    const send = (metric: RumMetric, value: number) =>
      dispatch({ metric, route, deviceTier, value }, true);
    if (lcp > 0) send('lcp', lcp);
    send('cls', maxCls * 1_000);
    const durations = [...interactions.values()].sort((left, right) => right - left);
    if (durations.length > 0) {
      const highPercentileIndex = Math.max(0, Math.ceil(durations.length * 0.02) - 1);
      send('inp', durations[highPercentileIndex]);
    }
  };
  const onVisibilityChange = () => {
    if (document.visibilityState === 'hidden') flush();
  };
  document.addEventListener('visibilitychange', onVisibilityChange, true);
  window.addEventListener('pagehide', flush, true);

  return () => {
    observers.forEach((observer) => observer.disconnect());
    document.removeEventListener('visibilitychange', onVisibilityChange, true);
    window.removeEventListener('pagehide', flush, true);
  };
}
