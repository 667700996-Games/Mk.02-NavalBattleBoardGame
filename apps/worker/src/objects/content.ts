import { DurableObject } from "cloudflare:workers";
import { DomainError } from "../domain/protocol";
import { bodyObject, json, requireString } from "../http";
import type { WorkerEnv } from "../env";

interface Season {
  id: string;
  title: string;
  description: string;
  startsAt: string;
  endsAt: string;
}

interface Event extends Season {}

interface FeatureFlags {
  missionsEnabled: boolean;
  eventBannerEnabled: boolean;
}

interface Tuning {
  dailyDeploymentRewardXp: number;
  dailyAccuracyRewardXp: number;
  weeklySupremacyRewardXp: number;
}

interface ContentPayload {
  activateAt: string;
  season: Season;
  events: Event[];
  featureFlags: FeatureFlags;
  tuning: Tuning;
  changeNote: string;
}

interface ContentRevision extends ContentPayload {
  schemaVersion: 1;
  revision: number;
  operatorId: string;
  createdAt: string;
  rolledBackFromRevision: number | null;
}

interface ContentState {
  revisions: ContentRevision[];
}

const BASELINE: ContentRevision = {
  schemaVersion: 1,
  revision: 0,
  activateAt: "2026-08-01T00:00:00.000Z",
  season: {
    id: "FOUNDERS_SEASON",
    title: "창립 함대 시즌",
    description: "정식 함대 지휘 체계를 확립하고 첫 시즌 전공을 기록하십시오.",
    startsAt: "2026-08-01T00:00:00.000Z",
    endsAt: "2026-10-31T23:59:59.000Z",
  },
  events: [
    {
      id: "COMMANDER_MUSTER",
      title: "지휘관 소집령",
      description:
        "일일·주간 임무를 완수해 창립 시즌 함대의 작전 기록을 확장하십시오.",
      startsAt: "2026-08-18T00:00:00.000Z",
      endsAt: "2026-09-01T00:00:00.000Z",
    },
  ],
  featureFlags: { missionsEnabled: true, eventBannerEnabled: true },
  tuning: {
    dailyDeploymentRewardXp: 100,
    dailyAccuracyRewardXp: 150,
    weeklySupremacyRewardXp: 400,
  },
  changeNote: "Built-in safe baseline content",
  operatorId: "SYSTEM_BASELINE",
  createdAt: "2026-08-01T00:00:00.000Z",
  rolledBackFromRevision: null,
};

export class ContentDurableObject extends DurableObject<WorkerEnv> {
  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method === "POST" && url.pathname === "/live")
        return json(await this.live(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/runtime")
        return json(await this.runtime(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/seasons")
        return json(await this.seasons(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/history")
        return json(await this.history(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/validate")
        return json(await this.validateCandidate(await bodyObject(request)));
      if (request.method === "POST" && url.pathname === "/publish")
        return json(await this.publish(await bodyObject(request)), 201);
      if (request.method === "POST" && url.pathname === "/rollback")
        return json(await this.rollback(await bodyObject(request)));
      return json({ code: "NOT_FOUND" }, 404);
    } catch (error) {
      const resolved =
        error instanceof DomainError
          ? error
          : new DomainError("INTERNAL_ERROR");
      return json(
        { code: resolved.code, message: resolved.message },
        resolved.code === "LIVE_CONTENT_REVISION_NOT_FOUND"
          ? 404
          : resolved.code === "LIVE_CONTENT_REVISION_CONFLICT"
            ? 409
            : resolved.code === "INTERNAL_ERROR"
              ? 500
              : 400,
      );
    }
  }

  private async live(input: Record<string, unknown>) {
    const now = validDate(input.now);
    const state = await this.read();
    return viewFor(activeRevision(state, now), now);
  }

  private async runtime(input: Record<string, unknown>) {
    const now = validDate(input.now);
    const state = await this.read();
    const revision = activeRevision(state, now);
    return { view: viewFor(revision, now), tuning: revision.tuning };
  }

  private async seasons(input: Record<string, unknown>) {
    const now = validDate(input.now);
    const state = await this.read();
    const active = activeRevision(state, now);
    const seasons = new Map<string, { seasonId: string; archived: boolean }>();
    for (const revision of [BASELINE, ...state.revisions]) {
      seasons.set(revision.season.id, {
        seasonId: revision.season.id,
        archived: revision.season.id !== active.season.id,
      });
    }
    return {
      currentSeasonId: active.season.id,
      seasons: [...seasons.values()],
    };
  }

  private async history(input: Record<string, unknown>) {
    const limit = Number(input.limit ?? 25);
    if (!Number.isInteger(limit) || limit < 1 || limit > 100)
      throw new DomainError("INVALID_REQUEST");
    const state = await this.read();
    const revisions = [...state.revisions]
      .sort((left, right) => right.revision - left.revision)
      .slice(0, limit);
    if (revisions.length < limit) revisions.push(structuredClone(BASELINE));
    return {
      currentRevision: latestRevision(state),
      revisions,
    };
  }

  private async validateCandidate(input: Record<string, unknown>) {
    const expectedRevision = nonnegativeInteger(input.expectedRevision);
    const operatorId = validOperator(input.operatorId);
    const now = validDate(input.now);
    const state = await this.read();
    if (latestRevision(state) !== expectedRevision)
      throw new DomainError("LIVE_CONTENT_REVISION_CONFLICT");
    const candidate = revisionFrom(
      expectedRevision + 1,
      payloadFrom(input.payload),
      operatorId,
      now,
      null,
    );
    return validationFor(candidate, now);
  }

  private async publish(input: Record<string, unknown>) {
    const expectedRevision = nonnegativeInteger(input.expectedRevision);
    const operatorId = validOperator(input.operatorId);
    const now = validDate(input.now);
    const payload = payloadFrom(input.payload);
    let published: ContentRevision | null = null;
    await this.mutate((state) => {
      if (latestRevision(state) !== expectedRevision)
        throw new DomainError("LIVE_CONTENT_REVISION_CONFLICT");
      const candidate = revisionFrom(
        expectedRevision + 1,
        payload,
        operatorId,
        now,
        null,
      );
      if (!validationFor(candidate, now).valid)
        throw new DomainError("INVALID_REQUEST");
      state.revisions.push(candidate);
      published = candidate;
    });
    if (!published) throw new DomainError("INTERNAL_ERROR");
    return published;
  }

  private async rollback(input: Record<string, unknown>) {
    const expectedRevision = nonnegativeInteger(input.expectedRevision);
    const targetRevision = nonnegativeInteger(input.targetRevision);
    if (targetRevision >= expectedRevision)
      throw new DomainError("INVALID_REQUEST");
    const operatorId = validOperator(input.operatorId);
    const now = validDate(input.now);
    const changeNote = requireString(input.changeNote);
    let rolledBack: ContentRevision | null = null;
    await this.mutate((state) => {
      if (latestRevision(state) !== expectedRevision)
        throw new DomainError("LIVE_CONTENT_REVISION_CONFLICT");
      const target =
        targetRevision === 0
          ? BASELINE
          : state.revisions.find(
              (revision) => revision.revision === targetRevision,
            );
      if (!target) throw new DomainError("LIVE_CONTENT_REVISION_NOT_FOUND");
      const candidate = revisionFrom(
        expectedRevision + 1,
        {
          activateAt: now,
          season: structuredClone(target.season),
          events: structuredClone(target.events),
          featureFlags: structuredClone(target.featureFlags),
          tuning: structuredClone(target.tuning),
          changeNote,
        },
        operatorId,
        now,
        targetRevision,
      );
      if (!validationFor(candidate, now).valid)
        throw new DomainError("INVALID_REQUEST");
      state.revisions.push(candidate);
      rolledBack = candidate;
    });
    if (!rolledBack) throw new DomainError("INTERNAL_ERROR");
    return rolledBack;
  }

  private async read(): Promise<ContentState> {
    const state =
      (await this.ctx.storage.get<ContentState>("state")) ??
      ({ revisions: [] } satisfies ContentState);
    state.revisions ??= [];
    return state;
  }

  private async mutate(action: (state: ContentState) => void): Promise<void> {
    await this.ctx.storage.transaction(async (transaction) => {
      const state =
        (await transaction.get<ContentState>("state")) ??
        ({ revisions: [] } satisfies ContentState);
      state.revisions ??= [];
      action(state);
      await transaction.put("state", state);
    });
  }
}

function activeRevision(state: ContentState, now: string): ContentRevision {
  return (
    [...state.revisions]
      .filter((revision) => revision.activateAt <= now)
      .sort((left, right) => right.revision - left.revision)[0] ?? BASELINE
  );
}

function latestRevision(state: ContentState): number {
  return state.revisions.at(-1)?.revision ?? 0;
}

function viewFor(revision: ContentRevision, now: string) {
  return {
    revision: revision.revision,
    season: {
      ...revision.season,
      status: temporalStatus(revision.season, now),
    },
    events: revision.featureFlags.eventBannerEnabled
      ? revision.events
          .filter((event) => temporalStatus(event, now) !== "ENDED")
          .map((event) => ({ ...event, status: temporalStatus(event, now) }))
      : [],
    featureFlags: revision.featureFlags,
    serverTime: now,
  };
}

function revisionFrom(
  revision: number,
  payload: ContentPayload,
  operatorId: string,
  createdAt: string,
  rolledBackFromRevision: number | null,
): ContentRevision {
  return {
    schemaVersion: 1,
    revision,
    ...structuredClone(payload),
    operatorId,
    createdAt,
    rolledBackFromRevision,
  };
}

function validationFor(revision: ContentRevision, now: string) {
  const issues: Array<{ code: string; path: string; message: string }> = [];
  const issue = (code: string, path: string, message: string) =>
    issues.push({ code, path, message });
  const activateOffset = Date.parse(revision.activateAt) - Date.parse(now);
  if (activateOffset < -5 * 60_000 || activateOffset > 90 * 86_400_000)
    issue(
      "UNSAFE_ACTIVATION_WINDOW",
      "activateAt",
      "활성화 시각은 현재 5분 전부터 90일 후 사이여야 합니다.",
    );
  if (!validCopy(revision.operatorId, 1, 64))
    issue(
      "INVALID_OPERATOR",
      "operatorId",
      "운영자 식별자가 올바르지 않습니다.",
    );
  if (!validCopy(revision.changeNote, 8, 256))
    issue(
      "INVALID_CHANGE_NOTE",
      "changeNote",
      "변경 사유는 8~256자여야 합니다.",
    );
  validateSeason(revision.season, "season", issue);
  const seasonDuration =
    Date.parse(revision.season.endsAt) - Date.parse(revision.season.startsAt);
  if (seasonDuration < 7 * 86_400_000 || seasonDuration > 200 * 86_400_000)
    issue("INVALID_SEASON_WINDOW", "season", "시즌은 7~200일 범위여야 합니다.");
  if (revision.season.endsAt <= revision.activateAt)
    issue(
      "EXPIRED_SEASON",
      "season.endsAt",
      "종료된 시즌은 발행할 수 없습니다.",
    );
  if (revision.events.length > 12)
    issue("TOO_MANY_EVENTS", "events", "이벤트는 최대 12개까지 허용됩니다.");
  const ids = new Set<string>();
  revision.events.forEach((event, index) => {
    validateSeason(event, `events[${index}]`, issue);
    if (ids.has(event.id))
      issue(
        "DUPLICATE_EVENT_ID",
        `events[${index}].id`,
        "이벤트 ID는 고유해야 합니다.",
      );
    ids.add(event.id);
    const duration = Date.parse(event.endsAt) - Date.parse(event.startsAt);
    if (duration <= 0 || duration > 45 * 86_400_000)
      issue(
        "INVALID_EVENT_WINDOW",
        `events[${index}].endsAt`,
        "이벤트 기간은 45일 이하여야 합니다.",
      );
    if (
      event.startsAt < revision.season.startsAt ||
      event.endsAt > revision.season.endsAt
    )
      issue(
        "EVENT_OUTSIDE_SEASON",
        `events[${index}]`,
        "이벤트는 시즌 기간 안에 있어야 합니다.",
      );
  });
  for (const [path, value, minimum, maximum] of [
    [
      "tuning.dailyDeploymentRewardXp",
      revision.tuning.dailyDeploymentRewardXp,
      25,
      500,
    ],
    [
      "tuning.dailyAccuracyRewardXp",
      revision.tuning.dailyAccuracyRewardXp,
      25,
      750,
    ],
    [
      "tuning.weeklySupremacyRewardXp",
      revision.tuning.weeklySupremacyRewardXp,
      100,
      2_500,
    ],
  ] as const) {
    if (!Number.isInteger(value) || value < minimum || value > maximum)
      issue("TUNING_OUT_OF_RANGE", path, "튜닝 값이 허용 범위를 벗어났습니다.");
  }
  return {
    valid: issues.length === 0,
    candidateRevision: revision.revision,
    issues,
  };
}

function validateSeason(
  value: Season,
  path: string,
  issue: (code: string, path: string, message: string) => void,
): void {
  if (!/^[A-Z0-9_]{3,32}$/.test(value.id))
    issue("INVALID_ID", `${path}.id`, "ID 형식이 올바르지 않습니다.");
  if (!validCopy(value.title, 2, 64))
    issue("INVALID_COPY", `${path}.title`, "제목 길이가 올바르지 않습니다.");
  if (!validCopy(value.description, 8, 240))
    issue(
      "INVALID_COPY",
      `${path}.description`,
      "설명 길이가 올바르지 않습니다.",
    );
}

function payloadFrom(value: unknown): ContentPayload {
  const payload = exactObject(value, [
    "activateAt",
    "season",
    "events",
    "featureFlags",
    "tuning",
    "changeNote",
  ]);
  if (!Array.isArray(payload.events)) throw new DomainError("INVALID_REQUEST");
  const flags = exactObject(payload.featureFlags, [
    "missionsEnabled",
    "eventBannerEnabled",
  ]);
  const tuning = exactObject(payload.tuning, [
    "dailyDeploymentRewardXp",
    "dailyAccuracyRewardXp",
    "weeklySupremacyRewardXp",
  ]);
  if (
    typeof flags.missionsEnabled !== "boolean" ||
    typeof flags.eventBannerEnabled !== "boolean"
  )
    throw new DomainError("INVALID_REQUEST");
  return {
    activateAt: validDate(payload.activateAt),
    season: seasonFrom(payload.season),
    events: payload.events.map(seasonFrom),
    featureFlags: {
      missionsEnabled: flags.missionsEnabled,
      eventBannerEnabled: flags.eventBannerEnabled,
    },
    tuning: {
      dailyDeploymentRewardXp: Number(tuning.dailyDeploymentRewardXp),
      dailyAccuracyRewardXp: Number(tuning.dailyAccuracyRewardXp),
      weeklySupremacyRewardXp: Number(tuning.weeklySupremacyRewardXp),
    },
    changeNote: requireString(payload.changeNote),
  };
}

function seasonFrom(value: unknown): Season {
  const season = exactObject(value, [
    "id",
    "title",
    "description",
    "startsAt",
    "endsAt",
  ]);
  return {
    id: requireString(season.id),
    title: requireString(season.title),
    description: requireString(season.description),
    startsAt: validDate(season.startsAt),
    endsAt: validDate(season.endsAt),
  };
}

function exactObject(value: unknown, keys: string[]): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new DomainError("INVALID_REQUEST");
  const object = value as Record<string, unknown>;
  if (
    Object.keys(object).length !== keys.length ||
    Object.keys(object).some((key) => !keys.includes(key)) ||
    keys.some((key) => !(key in object))
  )
    throw new DomainError("INVALID_REQUEST");
  return object;
}

function validDate(value: unknown): string {
  const date = requireString(value);
  if (!Number.isFinite(Date.parse(date)))
    throw new DomainError("INVALID_REQUEST");
  return new Date(date).toISOString();
}

function validOperator(value: unknown): string {
  const operator = requireString(value).trim();
  if (!/^[A-Za-z0-9._@-]{2,64}$/.test(operator))
    throw new DomainError("INVALID_REQUEST");
  return operator;
}

function nonnegativeInteger(value: unknown): number {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 0)
    throw new DomainError("INVALID_REQUEST");
  return number;
}

function validCopy(value: string, minimum: number, maximum: number): boolean {
  return (
    [...value].length >= minimum &&
    [...value].length <= maximum &&
    [...value.trim()].length >= minimum &&
    !/[\u0000-\u001f\u007f]/.test(value)
  );
}

function temporalStatus(value: Season, now: string) {
  if (now < value.startsAt) return "UPCOMING";
  if (now < value.endsAt) return "ACTIVE";
  return "ENDED";
}
