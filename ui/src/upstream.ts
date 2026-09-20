export function normalizeUpstreamBase(value: string): string {
  let normalized = value.trim().replace(/\/+$/, "");
  while (/\/v1$/i.test(normalized)) {
    normalized = normalized.slice(0, -3).replace(/\/+$/, "");
  }
  return normalized;
}
