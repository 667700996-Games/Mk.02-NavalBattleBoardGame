import {
  DomainError,
  MAX_PROTOCOL_VERSION,
  MIN_PROTOCOL_VERSION,
  PROTOCOL_CAPABILITIES,
  PROTOCOL_HEADERS,
  PROTOCOL_VERSION,
  protocolError,
  statusForError,
  type SessionRecord,
} from "./domain/protocol";

export const SESSION_COOKIE = "mk01_session";

export function json(
  value: unknown,
  status = 200,
  headers?: HeadersInit,
): Response {
  const responseHeaders = new Headers(headers);
  responseHeaders.set("content-type", "application/json; charset=utf-8");
  return new Response(JSON.stringify(value), {
    status,
    headers: responseHeaders,
  });
}

export function noContent(headers?: HeadersInit): Response {
  return new Response(null, { status: 204, headers });
}

export function errorResponse(error: unknown): Response {
  const payload = protocolError(error);
  return json(payload, statusForError(payload.code));
}

export async function bodyObject(
  request: Request,
): Promise<Record<string, unknown>> {
  const contentLength = Number(request.headers.get("content-length") ?? 0);
  if (contentLength > 64 * 1024) throw new DomainError("INVALID_REQUEST");
  try {
    const value: unknown = await request.json();
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new DomainError("INVALID_REQUEST");
    }
    return value as Record<string, unknown>;
  } catch (error) {
    if (error instanceof DomainError) throw error;
    throw new DomainError("INVALID_REQUEST");
  }
}

export function cookieValue(
  request: Request,
  name = SESSION_COOKIE,
): string | null {
  const cookies = request.headers.get("cookie") ?? "";
  for (const part of cookies.split(";")) {
    const separator = part.indexOf("=");
    if (separator < 0) continue;
    if (part.slice(0, separator).trim() === name) {
      return decodeURIComponent(part.slice(separator + 1).trim());
    }
  }
  return null;
}

export function sessionCookie(
  token: string,
  requestUrl: string,
  maxAgeSeconds: number,
): string {
  const secure = new URL(requestUrl).protocol === "https:" ? "; Secure" : "";
  return `${SESSION_COOKIE}=${encodeURIComponent(token)}; Path=/; HttpOnly; SameSite=Lax; Max-Age=${maxAgeSeconds}${secure}`;
}

export function expiredSessionCookie(requestUrl: string): string {
  return sessionCookie("", requestUrl, 0);
}

export function publicSession(session: SessionRecord) {
  return {
    id: session.id,
    accountId: session.accountId,
    nickname: session.nickname,
    currentRoomId: session.currentRoomId,
    expiresAt: session.expiresAt,
  };
}

export function withProtocolHeaders(
  response: Response,
  request: Request,
): Response {
  const requested = request.headers.get(PROTOCOL_HEADERS.version);
  const parsed = requested === null ? PROTOCOL_VERSION : Number(requested);
  if (
    !Number.isInteger(parsed) ||
    parsed < MIN_PROTOCOL_VERSION ||
    parsed > MAX_PROTOCOL_VERSION
  ) {
    return protocolMismatchResponse();
  }
  const headers = new Headers(response.headers);
  headers.set(PROTOCOL_HEADERS.version, String(parsed));
  headers.set(PROTOCOL_HEADERS.minimum, String(MIN_PROTOCOL_VERSION));
  headers.set(PROTOCOL_HEADERS.maximum, String(MAX_PROTOCOL_VERSION));
  headers.set(PROTOCOL_HEADERS.capabilities, PROTOCOL_CAPABILITIES.join(","));
  headers.set("x-content-type-options", "nosniff");
  headers.set("x-frame-options", "DENY");
  headers.set("referrer-policy", "strict-origin-when-cross-origin");
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

export function protocolMismatchResponse(): Response {
  const response = errorResponse(new DomainError("SERVER_PROTOCOL_MISMATCH"));
  const headers = new Headers(response.headers);
  headers.set(PROTOCOL_HEADERS.version, String(PROTOCOL_VERSION));
  headers.set(PROTOCOL_HEADERS.minimum, String(MIN_PROTOCOL_VERSION));
  headers.set(PROTOCOL_HEADERS.maximum, String(MAX_PROTOCOL_VERSION));
  headers.set(PROTOCOL_HEADERS.capabilities, PROTOCOL_CAPABILITIES.join(","));
  return new Response(response.body, { status: response.status, headers });
}

export function internalRequest(
  path: string,
  body?: unknown,
  headers?: HeadersInit,
): Request {
  const resolvedHeaders = new Headers(headers);
  if (body !== undefined)
    resolvedHeaders.set("content-type", "application/json");
  return new Request(`https://internal.mk01${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: resolvedHeaders,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

export function requireString(value: unknown): string {
  if (typeof value !== "string") throw new DomainError("INVALID_REQUEST");
  return value;
}

export function requireUuid(value: unknown): string {
  const text = requireString(value);
  if (
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
      text,
    )
  ) {
    throw new DomainError("INVALID_REQUEST");
  }
  return text;
}

export async function sha256(value: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export function randomSecret(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}
