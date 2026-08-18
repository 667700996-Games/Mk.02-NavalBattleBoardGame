import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$app/environment', () => ({ browser: true }));

interface FakeEntry {
  entryType: string;
  startTime: number;
  duration?: number;
  interactionId?: number;
  value?: number;
  hadRecentInput?: boolean;
}

class FakePerformanceObserver {
  static supportedEntryTypes = ['largest-contentful-paint', 'layout-shift', 'event'];
  static instances: FakePerformanceObserver[] = [];

  records: FakeEntry[] = [];
  observed?: PerformanceObserverInit;
  disconnected = false;

  constructor(private readonly callback: PerformanceObserverCallback) {
    FakePerformanceObserver.instances.push(this);
  }

  observe(options: PerformanceObserverInit) {
    this.observed = options;
  }

  takeRecords() {
    const records = this.records;
    this.records = [];
    return records as PerformanceEntryList;
  }

  disconnect() {
    this.disconnected = true;
  }

  emit(...entries: FakeEntry[]) {
    this.callback(
      { getEntries: () => entries as PerformanceEntryList } as PerformanceObserverEntryList,
      this as unknown as PerformanceObserver
    );
  }
}

const windowEvents = new EventTarget();
const documentEvents = new EventTarget();
const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(() =>
  Promise.resolve(new Response(null, { status: 204 }))
);
const sendBeacon = vi.fn<(url: string | URL, data?: BodyInit | null) => boolean>(() => false);
let now = 100;
let viewportWidth = 1_280;
let coarsePointer = false;
let visibilityState = 'visible';

beforeEach(() => {
  vi.useFakeTimers();
  FakePerformanceObserver.instances = [];
  fetchMock.mockClear();
  sendBeacon.mockClear();
  now = 100;
  viewportWidth = 1_280;
  coarsePointer = false;
  visibilityState = 'visible';

  vi.stubGlobal('navigator', {
    deviceMemory: 8,
    hardwareConcurrency: 8,
    sendBeacon
  });
  vi.stubGlobal('location', { pathname: '/room/FLEET1' });
  vi.stubGlobal('innerWidth', viewportWidth);
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({ matches: coarsePointer }))
  );
  vi.stubGlobal('fetch', fetchMock);
  vi.stubGlobal('performance', { now: () => now });
  vi.stubGlobal('PerformanceObserver', FakePerformanceObserver);
  vi.stubGlobal('window', {
    PerformanceObserver: FakePerformanceObserver,
    addEventListener: windowEvents.addEventListener.bind(windowEvents),
    removeEventListener: windowEvents.removeEventListener.bind(windowEvents)
  });
  vi.stubGlobal('document', {
    get visibilityState() {
      return visibilityState;
    },
    addEventListener: documentEvents.addEventListener.bind(documentEvents),
    removeEventListener: documentEvents.removeEventListener.bind(documentEvents)
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('browser RUM behavior', () => {
  it('classifies hardware tiers without leaking high-cardinality hardware details', async () => {
    const { detectRumDeviceTier } = await import('./performance');
    expect(detectRumDeviceTier()).toBe('desktop');

    vi.stubGlobal('innerWidth', 700);
    expect(detectRumDeviceTier()).toBe('mobile');

    vi.stubGlobal('innerWidth', 390);
    expect(detectRumDeviceTier()).toBe('low_mobile');

    vi.stubGlobal('innerWidth', 1_000);
    coarsePointer = true;
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => ({ matches: true }))
    );
    vi.stubGlobal('navigator', {
      deviceMemory: 4,
      hardwareConcurrency: 4,
      sendBeacon
    });
    expect(detectRumDeviceTier()).toBe('low_mobile');
  });

  it('bounds values, prefers an accepted beacon, and falls back to keepalive fetch', async () => {
    const { reportRumMetric } = await import('./performance');
    reportRumMetric('lcp', Number.NaN);
    reportRumMetric('cls', -1);
    expect(fetchMock).not.toHaveBeenCalled();

    reportRumMetric('lcp', 1_234.6);
    expect(fetchMock).toHaveBeenCalledOnce();
    const firstFetchInit = fetchMock.mock.calls[0]?.[1];
    expect(firstFetchInit).toBeDefined();
    expect(JSON.parse(firstFetchInit?.body as string)).toEqual({
      metric: 'lcp',
      route: 'room',
      deviceTier: 'desktop',
      value: 1_235
    });

    sendBeacon.mockReturnValueOnce(true);
    reportRumMetric('cls', 7.8, true);
    expect(sendBeacon).toHaveBeenCalledOnce();
    expect(fetchMock).toHaveBeenCalledOnce();

    sendBeacon.mockReturnValueOnce(false);
    reportRumMetric('inp', 95, true);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1]?.[1]).toMatchObject({ keepalive: true });
  });

  it('records, cancels, resolves, and expires battle interaction timers', async () => {
    const { cancelBattleInteraction, trackBattleInteractionResult, trackBattleInteractionStarted } =
      await import('./performance');

    trackBattleInteractionStarted('resolved');
    now = 143.7;
    trackBattleInteractionResult('resolved');
    expect(JSON.parse(fetchMock.mock.calls[0]?.[1]?.body as string)).toMatchObject({
      metric: 'battle_interaction',
      value: 44
    });

    trackBattleInteractionStarted('cancelled');
    cancelBattleInteraction('cancelled');
    trackBattleInteractionResult('cancelled');
    expect(fetchMock).toHaveBeenCalledOnce();

    trackBattleInteractionStarted('expired');
    vi.advanceTimersByTime(60_000);
    trackBattleInteractionResult('expired');
    trackBattleInteractionResult('unknown');
    expect(fetchMock).toHaveBeenCalledOnce();
  });

  it('flushes LCP, CLS, and high-tail INP once and disconnects every observer', async () => {
    const { installRealUserMonitoring } = await import('./performance');
    const cleanup = installRealUserMonitoring();
    expect(FakePerformanceObserver.instances).toHaveLength(3);

    const [lcp, cls, event] = FakePerformanceObserver.instances;
    lcp.emit({ entryType: 'largest-contentful-paint', startTime: 1_250 });
    cls.emit(
      { entryType: 'layout-shift', startTime: 100, value: 0.03, hadRecentInput: false },
      { entryType: 'layout-shift', startTime: 600, value: 0.04, hadRecentInput: false },
      { entryType: 'layout-shift', startTime: 700, value: 0.9, hadRecentInput: true },
      { entryType: 'layout-shift', startTime: 6_200, value: 0.02, hadRecentInput: false }
    );
    event.emit(
      { entryType: 'event', startTime: 0, duration: 80, interactionId: 1 },
      { entryType: 'event', startTime: 0, duration: 120, interactionId: 1 },
      { entryType: 'event', startTime: 0, duration: 50, interactionId: 2 },
      { entryType: 'event', startTime: 0, duration: 999, interactionId: 0 }
    );

    visibilityState = 'hidden';
    documentEvents.dispatchEvent(new Event('visibilitychange'));
    windowEvents.dispatchEvent(new Event('pagehide'));

    expect(sendBeacon).toHaveBeenCalledTimes(3);
    const payloads = await Promise.all(
      sendBeacon.mock.calls.map(async (call) => {
        expect(call[1]).toBeInstanceOf(Blob);
        return JSON.parse(await (call[1] as Blob).text());
      })
    );
    expect(payloads).toEqual([
      expect.objectContaining({ metric: 'lcp', value: 1_250 }),
      expect.objectContaining({ metric: 'cls', value: 70 }),
      expect.objectContaining({ metric: 'inp', value: 120 })
    ]);

    cleanup();
    expect(FakePerformanceObserver.instances.every((observer) => observer.disconnected)).toBe(true);
  });

  it('ignores unsupported observer types and tolerates an observer setup failure', async () => {
    const original = FakePerformanceObserver.prototype.observe;
    FakePerformanceObserver.supportedEntryTypes = ['largest-contentful-paint', 'event'];
    FakePerformanceObserver.prototype.observe = function (options) {
      if (options.type === 'event') throw new Error('unsupported options');
      return original.call(this, options);
    };
    const { installRealUserMonitoring } = await import('./performance');
    const cleanup = installRealUserMonitoring();
    expect(FakePerformanceObserver.instances).toHaveLength(2);
    cleanup();
    FakePerformanceObserver.prototype.observe = original;
    FakePerformanceObserver.supportedEntryTypes = [
      'largest-contentful-paint',
      'layout-shift',
      'event'
    ];
  });
});
