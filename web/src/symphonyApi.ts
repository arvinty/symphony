const configuredBase = import.meta.env.VITE_SYMPHONY_API_BASE?.replace(/\/+$/, "") ?? "";

export function symphonyApiUrl(path: string): string {
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;
  return `${configuredBase}${normalizedPath}`;
}
