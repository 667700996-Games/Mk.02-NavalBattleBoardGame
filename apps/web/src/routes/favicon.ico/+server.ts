const favicon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <defs>
    <radialGradient id="ocean" cx="50%" cy="42%" r="68%">
      <stop offset="0" stop-color="#12394a" />
      <stop offset="1" stop-color="#030b11" />
    </radialGradient>
  </defs>
  <rect width="64" height="64" rx="14" fill="url(#ocean)" />
  <circle cx="32" cy="32" r="20" fill="none" stroke="#28dfe8" stroke-width="3" />
  <circle cx="32" cy="32" r="9" fill="none" stroke="#8effff" stroke-width="2" />
  <path d="M32 5v14M32 45v14M5 32h14M45 32h14" stroke="#53e9e8" stroke-width="3" stroke-linecap="round" />
  <circle cx="32" cy="32" r="3" fill="#8effff" />
</svg>`;

export function GET(): Response {
  return new Response(favicon, {
    headers: {
      'cache-control': 'public, max-age=86400',
      'content-type': 'image/svg+xml; charset=utf-8'
    }
  });
}
