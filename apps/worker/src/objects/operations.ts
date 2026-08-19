import { DurableObject } from "cloudflare:workers";
import { DomainError, type GameResult } from "../domain/protocol";
import {
  bodyObject,
  json,
  noContent,
  requireString,
  requireUuid,
} from "../http";
import type { WorkerEnv } from "../env";

type ReportStatus = "OPEN" | "REVIEWING" | "ACTIONED" | "DISMISSED";
type ReportCategory = "CHAT" | "NAME" | "CHEATING" | "STALLING" | "OTHER";
type ModerationActionKind = "WARN" | "SUSPEND" | "BAN" | "DISMISS" | "REVERSE";
type IntegrityKind =
  "IMPOSSIBLE_ORDER" | "AUTOMATION" | "COLLUSION" | "INTENTIONAL_STALLING";

interface PlayerReport {
  id: string;
  reporterIdentityId: string;
  targetIdentityId: string;
  roomId: string;
  targetPlayerId: string;
  targetNickname: string;
  category: ReportCategory;
  details: string;
  evidence: Record<string, unknown>;
  status: ReportStatus;
  createdAt: string;
  updatedAt: string;
}

interface ModerationAction {
  id: string;
  reportId: string;
  targetIdentityId: string;
  operatorId: string;
  action: ModerationActionKind;
  reason: string;
  expiresAt: string | null;
  reversesActionId: string | null;
  createdAt: string;
}

interface IntegritySignal {
  id: string;
  subjectIdentityId: string;
  roomId: string | null;
  kind: IntegrityKind;
  severity: number;
  confidence: number;
  evidence: Record<string, unknown>;
  occurrences: number;
  firstObservedAt: string;
  lastObservedAt: string;
}

interface SupportAction {
  id: string;
  accountId: string;
  operatorId: string;
  action: "REVOKE_SESSION" | "REVOKE_ALL_SESSIONS";
  reason: string;
  targetSessionId: string | null;
  affectedSessionIds: string[];
  createdAt: string;
}

interface RumDistribution {
  count: number;
  sum: number;
  buckets: number[];
}

interface TelemetryState {
  funnelEvents: Record<string, number>;
  funnelFailures: Record<string, number>;
  rum: Record<string, RumDistribution>;
}

interface OperationsState {
  reports: Record<string, PlayerReport>;
  moderationActions: Record<string, ModerationAction>;
  signals: Record<string, IntegritySignal>;
  supportActions: Record<string, SupportAction>;
  shortMatches: Record<string, string[]>;
  telemetry: TelemetryState;
}

const EMPTY_STATE: OperationsState = {
  reports: {},
  moderationActions: {},
  signals: {},
  supportActions: {},
  shortMatches: {},
  telemetry: { funnelEvents: {}, funnelFailures: {}, rum: {} },
};

const FUNNEL_STAGES = [
  "landing",
  "tutorial_started",
  "tutorial_completed",
  "session_created",
  "lobby_entered",
  "room_joined",
  "placement_completed",
  "first_attack",
  "match_completed",
] as const;
const FUNNEL_OUTCOMES = ["reached", "failed", "abandoned"] as const;
const FUNNEL_FAILURES = [
  "network",
  "session_creation",
  "authentication",
  "room_entry",
  "matchmaking",
  "recovery",
  "placement",
  "attack",
] as const;
const RUM_ROUTES = [
  "landing",
  "tutorial",
  "lobby",
  "join",
  "room",
  "account",
  "replay",
  "other",
] as const;
const RUM_DEVICE_TIERS = ["desktop", "mobile", "low_mobile"] as const;
const RUM_METRICS = {
  lcp: {
    name: "mk01_rum_lcp_milliseconds",
    help: "Real-user Largest Contentful Paint in milliseconds.",
    maximum: 60_000,
    buckets: [1_000, 2_500, 4_000, 8_000, 16_000],
  },
  cls: {
    name: "mk01_rum_cls_milli",
    help: "Real-user Cumulative Layout Shift multiplied by 1000.",
    maximum: 5_000,
    buckets: [50, 100, 250, 500, 1_000],
  },
  inp: {
    name: "mk01_rum_inp_milliseconds",
    help: "Real-user Interaction to Next Paint in milliseconds.",
    maximum: 30_000,
    buckets: [100, 200, 500, 1_000, 2_500],
  },
  battle_interaction: {
    name: "mk01_rum_battle_interaction_milliseconds",
    help: "Real-user attack command to authoritative result latency in milliseconds.",
    maximum: 60_000,
    buckets: [100, 200, 500, 1_000, 2_500],
  },
} as const;

export class OperationsDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/reports/create")
        return await this.createReport(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/reports/list")
        return json(await this.listReports(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/reports/moderate")
        return json(await this.moderate(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/penalty")
        return json(await this.activePenalty(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/integrity/record")
        return json(await this.recordSignal(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/integrity/assess")
        return await this.assessResult(await bodyObject(request));
      if (request.method === "POST" && url.pathname === "/integrity/list")
        return json(await this.listSignals(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/support/record")
        return json(await this.recordSupportAction(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/support/actions")
        return json(await this.supportActions(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/telemetry/funnel")
        return await this.recordFunnel(await bodyObject(request));
      if (
        request.method === "POST" &&
        url.pathname === "/telemetry/performance"
      )
        return await this.recordPerformance(await bodyObject(request));
      if (request.method === "GET" && url.pathname === "/metrics")
        return new Response(await this.renderMetrics(), {
          headers: { "content-type": "text/plain; version=0.0.4" },
        });
      if (request.method === "GET" && url.pathname === "/health")
        return json({ status: "ok" });
      if (request.method === "POST" && url.pathname === "/export")
        return json(await this.exportData(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/delete")
        return json(await this.deleteIdentity(await bodyObject(request)));
      return json({ code: "NOT_FOUND" }, 404);
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INTERNAL_ERROR");
      return json(
        { code: resolved.code, message: resolved.message },
        resolved.code === "REPORT_NOT_FOUND"
          ? 404
          : resolved.code === "INTERNAL_ERROR"
            ? 500
            : 400,
      );
    }
  }

  private async createReport(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const details = requireString(input.details).trim();
    if (
      [...details].length < 4 ||
      [...details].length > 1_000 ||
      [...details].some((character) =>
        /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(character),
      )
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    const category = requireString(input.category) as ReportCategory;
    if (!["CHAT", "NAME", "CHEATING", "STALLING", "OTHER"].includes(category))
      throw new DomainError("INVALID_REQUEST");
    const now = requireString(input.now);
    const report: PlayerReport = {
      id: crypto.randomUUID(),
      reporterIdentityId: requireUuid(input.reporterIdentityId),
      targetIdentityId: requireUuid(input.targetIdentityId),
      roomId: requireUuid(input.roomId),
      targetPlayerId: requireUuid(input.targetPlayerId),
      targetNickname: requireString(input.targetNickname),
      category,
      details,
      evidence: objectValue(input.evidence),
      status: "OPEN",
      createdAt: now,
      updatedAt: now,
    };
    await this.mutate((state) => {
      state.reports[report.id] = report;
    });
    return json(
      { reportId: report.id, status: "OPEN", createdAt: report.createdAt },
      201,
    );
  }

  private async listReports(input: Record<string, unknown>) {
    const status =
      input.status === null || input.status === undefined
        ? null
        : (requireString(input.status) as ReportStatus);
    if (
      status &&
      !["OPEN", "REVIEWING", "ACTIONED", "DISMISSED"].includes(status)
    )
      throw new DomainError("INVALID_REQUEST");
    const search = optionalSearch(input.search);
    const before = optionalDate(input.before);
    const limit = boundedLimit(input.limit);
    const state = await this.read();
    const reports = Object.values(state.reports)
      .filter((report) => !status || report.status === status)
      .filter((report) => !before || report.createdAt < before)
      .filter(
        (report) =>
          !search ||
          `${report.targetNickname} ${report.details} ${JSON.stringify(report.evidence)}`
            .toLocaleLowerCase()
            .includes(search),
      )
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt));
    const cases = reports.slice(0, limit).map((report) => ({
      report,
      actions: Object.values(state.moderationActions)
        .filter((action) => action.reportId === report.id)
        .sort((left, right) => left.createdAt.localeCompare(right.createdAt)),
    }));
    return {
      cases,
      nextBefore:
        reports.length > limit
          ? (cases.at(-1)?.report.createdAt ?? null)
          : null,
    };
  }

  private async moderate(input: Record<string, unknown>) {
    const reportId = requireUuid(input.reportId);
    const operatorId = validOperator(input.operatorId);
    const action = requireString(input.action) as ModerationActionKind;
    if (!["WARN", "SUSPEND", "BAN", "DISMISS", "REVERSE"].includes(action))
      throw new DomainError("INVALID_REQUEST");
    const reason = requireString(input.reason).trim();
    if (
      [...reason].length < 4 ||
      [...reason].length > 1_000 ||
      [...reason].some((character) => /[\u0000-\u001f\u007f]/.test(character))
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    const now = requireString(input.now);
    const durationHours =
      input.durationHours === null || input.durationHours === undefined
        ? null
        : Number(input.durationHours);
    const reversesActionId =
      input.reversesActionId === null || input.reversesActionId === undefined
        ? null
        : requireUuid(input.reversesActionId);
    let expiresAt: string | null = null;
    if (action === "SUSPEND") {
      if (
        !Number.isInteger(durationHours) ||
        durationHours === null ||
        durationHours < 1 ||
        durationHours > 8_760 ||
        reversesActionId
      ) {
        throw new DomainError("INVALID_REQUEST");
      }
      expiresAt = new Date(
        Date.parse(now) + durationHours * 3_600_000,
      ).toISOString();
    } else if (action === "REVERSE") {
      if (durationHours !== null || !reversesActionId)
        throw new DomainError("INVALID_REQUEST");
    } else if (durationHours !== null || reversesActionId) {
      throw new DomainError("INVALID_REQUEST");
    }
    let stored: ModerationAction | null = null;
    await this.mutate((state) => {
      const report = state.reports[reportId];
      if (!report) throw new DomainError("REPORT_NOT_FOUND");
      if (action === "REVERSE") {
        const reversed = state.moderationActions[reversesActionId!];
        if (
          !reversed ||
          reversed.reportId !== report.id ||
          reversed.targetIdentityId !== report.targetIdentityId ||
          ["REVERSE", "DISMISS"].includes(reversed.action) ||
          Object.values(state.moderationActions).some(
            (candidate) => candidate.reversesActionId === reversed.id,
          )
        ) {
          throw new DomainError("INVALID_REQUEST");
        }
      }
      stored = {
        id: crypto.randomUUID(),
        reportId,
        targetIdentityId: report.targetIdentityId,
        operatorId,
        action,
        reason,
        expiresAt,
        reversesActionId,
        createdAt: now,
      };
      state.moderationActions[stored.id] = stored;
      report.status =
        action === "DISMISS"
          ? "DISMISSED"
          : action === "REVERSE"
            ? "REVIEWING"
            : "ACTIONED";
      report.updatedAt = now;
    });
    if (!stored) throw new DomainError("INTERNAL_ERROR");
    return { action: stored };
  }

  private async activePenalty(input: Record<string, unknown>) {
    const identityId = requireUuid(input.identityId);
    const sessionId = requireUuid(input.sessionId);
    const now = requireString(input.now);
    const state = await this.read();
    const reversed = new Set(
      Object.values(state.moderationActions)
        .map((action) => action.reversesActionId)
        .filter((id): id is string => Boolean(id)),
    );
    const active = Object.values(state.moderationActions).filter(
      (action) =>
        [identityId, sessionId].includes(action.targetIdentityId) &&
        !reversed.has(action.id),
    );
    if (active.some((action) => action.action === "BAN"))
      return { penalty: "BANNED", expiresAt: null };
    const expiresAt = active
      .filter(
        (action) =>
          action.action === "SUSPEND" &&
          action.expiresAt !== null &&
          action.expiresAt > now,
      )
      .map((action) => action.expiresAt!)
      .sort()
      .at(-1);
    return expiresAt
      ? { penalty: "SUSPENDED", expiresAt }
      : { penalty: null, expiresAt: null };
  }

  private async recordSignal(input: Record<string, unknown>) {
    const signal = signalFrom(input);
    let stored: IntegritySignal | null = null;
    await this.mutate((state) => {
      const existing = Object.values(state.signals).find(
        (candidate) =>
          signal.roomId !== null &&
          candidate.subjectIdentityId === signal.subjectIdentityId &&
          candidate.roomId === signal.roomId &&
          candidate.kind === signal.kind,
      );
      if (existing) {
        existing.severity = Math.max(existing.severity, signal.severity);
        existing.confidence = Math.max(existing.confidence, signal.confidence);
        existing.evidence = signal.evidence;
        existing.occurrences += 1;
        existing.lastObservedAt = signal.lastObservedAt;
        stored = structuredClone(existing);
      } else {
        state.signals[signal.id] = signal;
        stored = signal;
      }
    });
    if (!stored) throw new DomainError("INTERNAL_ERROR");
    return stored;
  }

  private async assessResult(
    input: Record<string, unknown>,
  ): Promise<Response> {
    const roomId = requireUuid(input.roomId);
    const gameId = requireUuid(input.gameId);
    const result = input.result as GameResult;
    const participants = participantList(input.participants);
    const now = requireString(input.now);
    for (const participant of participants) {
      const statistics = result.players.find(
        (player) => player.playerId === participant.playerId,
      );
      if (
        (statistics?.totalTimeouts ?? 0) >= 3 ||
        (result.finishReason === "TURN_TIMEOUT" &&
          result.loserId === participant.playerId)
      ) {
        await this.recordSignal({
          subjectIdentityId: participant.identityId,
          roomId,
          kind: "INTENTIONAL_STALLING",
          severity: result.finishReason === "TURN_TIMEOUT" ? 4 : 3,
          confidence: 0.92,
          evidence: {
            protocolVersion: 3,
            gameId,
            playerId: participant.playerId,
            totalTimeouts: statistics?.totalTimeouts ?? 0,
            finishReason: result.finishReason,
            totalTurns: result.totalTurns,
          },
          now,
        });
      }
    }
    if (
      participants.length === 2 &&
      result.totalTurns <= 5 &&
      result.finishReason !== "FLEET_DESTROYED"
    ) {
      const key = participants
        .map((participant) => participant.identityId)
        .sort()
        .join(":");
      let count = 0;
      await this.mutate((state) => {
        state.shortMatches[key] = (state.shortMatches[key] ?? []).filter(
          (timestamp) =>
            Date.parse(now) - Date.parse(timestamp) < 7 * 86_400_000,
        );
        if (!state.shortMatches[key].includes(result.finishedAt))
          state.shortMatches[key].push(result.finishedAt);
        count = state.shortMatches[key].length;
      });
      if (count >= 3) {
        for (const participant of participants) {
          await this.recordSignal({
            subjectIdentityId: participant.identityId,
            roomId,
            kind: "COLLUSION",
            severity: 4,
            confidence: 0.82,
            evidence: {
              protocolVersion: 3,
              gameId,
              pairedIdentityIds: participants.map((item) => item.identityId),
              suspiciousShortMatchesSevenDays: count,
              finishReason: result.finishReason,
              totalTurns: result.totalTurns,
            },
            now,
          });
        }
      }
    }
    return noContent();
  }

  private async listSignals(input: Record<string, unknown>) {
    const kind =
      input.kind === null || input.kind === undefined
        ? null
        : (requireString(input.kind) as IntegrityKind);
    if (
      kind &&
      ![
        "IMPOSSIBLE_ORDER",
        "AUTOMATION",
        "COLLUSION",
        "INTENTIONAL_STALLING",
      ].includes(kind)
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    const search = optionalSearch(input.search);
    const before = optionalDate(input.before);
    const limit = boundedLimit(input.limit);
    const state = await this.read();
    const signals = Object.values(state.signals)
      .filter((signal) => !kind || signal.kind === kind)
      .filter((signal) => !before || signal.lastObservedAt < before)
      .filter(
        (signal) =>
          !search ||
          `${signal.subjectIdentityId} ${JSON.stringify(signal.evidence)}`
            .toLocaleLowerCase()
            .includes(search),
      )
      .sort((left, right) =>
        right.lastObservedAt.localeCompare(left.lastObservedAt),
      );
    const page = signals.slice(0, limit);
    return {
      signals: page,
      nextBefore:
        signals.length > limit ? (page.at(-1)?.lastObservedAt ?? null) : null,
    };
  }

  private async recordSupportAction(input: Record<string, unknown>) {
    const operatorId = validOperator(input.operatorId);
    const reason = requireString(input.reason).trim();
    if (
      [...reason].length < 8 ||
      [...reason].length > 500 ||
      [...reason].some((character) =>
        /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(character),
      )
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    if (!Array.isArray(input.affectedSessionIds))
      throw new DomainError("INVALID_REQUEST");
    const targetSessionId =
      input.targetSessionId === null
        ? null
        : requireUuid(input.targetSessionId);
    const action: SupportAction = {
      id: crypto.randomUUID(),
      accountId: requireUuid(input.accountId),
      operatorId,
      action: targetSessionId ? "REVOKE_SESSION" : "REVOKE_ALL_SESSIONS",
      reason,
      targetSessionId,
      affectedSessionIds: input.affectedSessionIds.map(requireUuid),
      createdAt: requireString(input.now),
    };
    await this.mutate((state) => {
      state.supportActions[action.id] = action;
    });
    return { action };
  }

  private async supportActions(input: Record<string, unknown>) {
    const accountId = requireUuid(input.accountId);
    const state = await this.read();
    return {
      actions: Object.values(state.supportActions)
        .filter((action) => action.accountId === accountId)
        .sort((left, right) => right.createdAt.localeCompare(left.createdAt)),
    };
  }

  private async recordFunnel(
    input: Record<string, unknown>,
  ): Promise<Response> {
    requireExactKeys(input, ["stage", "outcome", "reason"]);
    const stage = requireString(input.stage);
    const outcome = requireString(input.outcome);
    const reason =
      input.reason === undefined || input.reason === null
        ? null
        : requireString(input.reason);
    if (
      !includes(FUNNEL_STAGES, stage) ||
      !includes(FUNNEL_OUTCOMES, outcome) ||
      (outcome === "failed") !== (reason !== null) ||
      (reason !== null && !includes(FUNNEL_FAILURES, reason))
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    await this.mutate((state) => {
      const key = `${stage}:${outcome}`;
      state.telemetry.funnelEvents[key] =
        (state.telemetry.funnelEvents[key] ?? 0) + 1;
      if (reason)
        state.telemetry.funnelFailures[reason] =
          (state.telemetry.funnelFailures[reason] ?? 0) + 1;
    });
    return noContent();
  }

  private async recordPerformance(
    input: Record<string, unknown>,
  ): Promise<Response> {
    requireExactKeys(input, ["metric", "route", "deviceTier", "value"]);
    const metric = requireString(input.metric);
    const route = requireString(input.route);
    const deviceTier = requireString(input.deviceTier);
    const value = Number(input.value);
    if (
      !Object.hasOwn(RUM_METRICS, metric) ||
      !includes(RUM_ROUTES, route) ||
      !includes(RUM_DEVICE_TIERS, deviceTier) ||
      !Number.isInteger(value) ||
      value < 0
    ) {
      throw new DomainError("INVALID_REQUEST");
    }
    const definition = RUM_METRICS[metric as keyof typeof RUM_METRICS];
    if (value > definition.maximum) throw new DomainError("INVALID_REQUEST");
    await this.mutate((state) => {
      const key = `${metric}:${route}:${deviceTier}`;
      const distribution = (state.telemetry.rum[key] ??= {
        count: 0,
        sum: 0,
        buckets: definition.buckets.map(() => 0),
      });
      distribution.count += 1;
      distribution.sum += value;
      definition.buckets.forEach((upperBound, index) => {
        if (value <= upperBound) distribution.buckets[index] += 1;
      });
    });
    return noContent();
  }

  private async renderMetrics(): Promise<string> {
    const state = await this.read();
    let output =
      "# HELP mk01_new_player_funnel_events_total Aggregate onboarding events by fixed stage and outcome.\n" +
      "# TYPE mk01_new_player_funnel_events_total counter\n";
    for (const stage of FUNNEL_STAGES) {
      for (const outcome of FUNNEL_OUTCOMES) {
        output += `mk01_new_player_funnel_events_total{stage="${stage}",outcome="${outcome}"} ${state.telemetry.funnelEvents[`${stage}:${outcome}`] ?? 0}\n`;
      }
    }
    output +=
      "# HELP mk01_new_player_funnel_failures_total Aggregate onboarding failures by fixed reason.\n" +
      "# TYPE mk01_new_player_funnel_failures_total counter\n";
    for (const reason of FUNNEL_FAILURES) {
      output += `mk01_new_player_funnel_failures_total{reason="${reason}"} ${state.telemetry.funnelFailures[reason] ?? 0}\n`;
    }
    for (const [metric, definition] of Object.entries(RUM_METRICS)) {
      output += `# HELP ${definition.name} ${definition.help}\n# TYPE ${definition.name} histogram\n`;
      for (const route of RUM_ROUTES) {
        for (const deviceTier of RUM_DEVICE_TIERS) {
          const distribution =
            state.telemetry.rum[`${metric}:${route}:${deviceTier}`];
          if (!distribution?.count) continue;
          definition.buckets.forEach((upperBound, index) => {
            output += `${definition.name}_bucket{route="${route}",device_tier="${deviceTier}",le="${upperBound}"} ${distribution.buckets[index] ?? 0}\n`;
          });
          output += `${definition.name}_bucket{route="${route}",device_tier="${deviceTier}",le="+Inf"} ${distribution.count}\n`;
          output += `${definition.name}_sum{route="${route}",device_tier="${deviceTier}"} ${distribution.sum}\n`;
          output += `${definition.name}_count{route="${route}",device_tier="${deviceTier}"} ${distribution.count}\n`;
        }
      }
    }
    output += `# HELP mk01_moderation_reports_total Persisted player reports.\n# TYPE mk01_moderation_reports_total gauge\nmk01_moderation_reports_total ${Object.keys(state.reports).length}\n`;
    output += `# HELP mk01_integrity_signals_total Persisted integrity signals.\n# TYPE mk01_integrity_signals_total gauge\nmk01_integrity_signals_total ${Object.keys(state.signals).length}\n`;
    return output;
  }

  private async exportData(input: Record<string, unknown>) {
    const identityId = requireUuid(input.identityId);
    const state = await this.read();
    const reports = Object.values(state.reports).filter(
      (report) =>
        report.reporterIdentityId === identityId ||
        report.targetIdentityId === identityId,
    );
    const reportIds = new Set(reports.map((report) => report.id));
    return {
      moderationReports: reports,
      moderationActions: Object.values(state.moderationActions).filter(
        (action) =>
          action.targetIdentityId === identityId ||
          reportIds.has(action.reportId),
      ),
      integritySignals: Object.values(state.signals).filter(
        (signal) => signal.subjectIdentityId === identityId,
      ),
      supportActions: Object.values(state.supportActions).filter(
        (action) => action.accountId === identityId,
      ),
    };
  }

  private async deleteIdentity(input: Record<string, unknown>) {
    const identityId = requireUuid(input.identityId);
    let reportsDeleted = 0;
    let signalsDeleted = 0;
    await this.mutate((state) => {
      const reportIds = Object.values(state.reports)
        .filter(
          (report) =>
            report.reporterIdentityId === identityId ||
            report.targetIdentityId === identityId,
        )
        .map((report) => report.id);
      reportsDeleted = reportIds.length;
      for (const reportId of reportIds) delete state.reports[reportId];
      for (const [actionId, action] of Object.entries(
        state.moderationActions,
      )) {
        if (
          action.targetIdentityId === identityId ||
          reportIds.includes(action.reportId)
        )
          delete state.moderationActions[actionId];
      }
      for (const [signalId, signal] of Object.entries(state.signals)) {
        if (signal.subjectIdentityId === identityId) {
          delete state.signals[signalId];
          signalsDeleted += 1;
        }
      }
      for (const [actionId, action] of Object.entries(state.supportActions)) {
        if (action.accountId === identityId)
          delete state.supportActions[actionId];
      }
      for (const key of Object.keys(state.shortMatches)) {
        if (key.split(":").includes(identityId)) delete state.shortMatches[key];
      }
    });
    return { reportsDeleted, integritySignalsDeleted: signalsDeleted };
  }

  private async read(): Promise<OperationsState> {
    const state =
      (await this.ctx.storage.get<OperationsState>("state")) ??
      structuredClone(EMPTY_STATE);
    normalize(state);
    return state;
  }

  private async mutate(
    action: (state: OperationsState) => void,
  ): Promise<void> {
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<OperationsState>("state")) ??
        structuredClone(EMPTY_STATE);
      normalize(state);
      action(state);
      await transaction.put("state", state);
    });
  }
}

function signalFrom(input: Record<string, unknown>): IntegritySignal {
  const kind = requireString(input.kind) as IntegrityKind;
  if (
    ![
      "IMPOSSIBLE_ORDER",
      "AUTOMATION",
      "COLLUSION",
      "INTENTIONAL_STALLING",
    ].includes(kind)
  ) {
    throw new DomainError("INVALID_REQUEST");
  }
  const severity = Number(input.severity);
  const confidence = Number(input.confidence);
  if (
    !Number.isFinite(severity) ||
    !Number.isFinite(confidence) ||
    confidence < 0 ||
    confidence > 1
  ) {
    throw new DomainError("INVALID_REQUEST");
  }
  const now = requireString(input.now);
  return {
    id: crypto.randomUUID(),
    subjectIdentityId: requireUuid(input.subjectIdentityId),
    roomId: input.roomId === null ? null : requireUuid(input.roomId),
    kind,
    severity: Math.max(1, Math.min(5, Math.round(severity))),
    confidence,
    evidence: objectValue(input.evidence),
    occurrences: 1,
    firstObservedAt: now,
    lastObservedAt: now,
  };
}

function participantList(value: unknown) {
  if (!Array.isArray(value) || value.length < 1 || value.length > 2)
    throw new DomainError("INVALID_REQUEST");
  return value.map((entry) => {
    const participant = objectValue(entry);
    return {
      identityId: requireUuid(participant.identityId),
      playerId: requireUuid(participant.playerId),
    };
  });
}

function objectValue(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new DomainError("INVALID_REQUEST");
  return structuredClone(value) as Record<string, unknown>;
}

function validOperator(value: unknown): string {
  const operator = requireString(value).trim();
  if (!/^[A-Za-z0-9._@-]{2,64}$/.test(operator))
    throw new DomainError("INVALID_REQUEST");
  return operator;
}

function optionalSearch(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  const search = requireString(value).trim().toLocaleLowerCase();
  if (search.length > 128) throw new DomainError("INVALID_REQUEST");
  return search || null;
}

function optionalDate(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  const date = requireString(value);
  if (!Number.isFinite(Date.parse(date)))
    throw new DomainError("INVALID_REQUEST");
  return date;
}

function boundedLimit(value: unknown): number {
  const limit = value === null || value === undefined ? 25 : Number(value);
  if (!Number.isInteger(limit)) throw new DomainError("INVALID_REQUEST");
  return Math.max(1, Math.min(100, limit));
}

function normalize(state: OperationsState): void {
  state.reports ??= {};
  state.moderationActions ??= {};
  state.signals ??= {};
  state.supportActions ??= {};
  state.shortMatches ??= {};
  state.telemetry ??= { funnelEvents: {}, funnelFailures: {}, rum: {} };
  state.telemetry.funnelEvents ??= {};
  state.telemetry.funnelFailures ??= {};
  state.telemetry.rum ??= {};
}

function requireExactKeys(
  input: Record<string, unknown>,
  allowed: readonly string[],
): void {
  if (Object.keys(input).some((key) => !allowed.includes(key)))
    throw new DomainError("INVALID_REQUEST");
}

function includes(values: readonly string[], value: string): boolean {
  return values.includes(value);
}
