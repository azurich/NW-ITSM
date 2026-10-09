/** Formats an API timestamp (RFC 3339) for display, or returns `fallback`. */
export function formatDateTime(value: string | null | undefined, locale: string, fallback = '—'): string {
  if (!value) {
    return fallback
  }
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return fallback
  }
  return new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'short' }).format(date)
}

/** URL of a page of the legacy PHP application. */
export function legacyUrl(path: string): string {
  const base = (import.meta.env.VITE_LEGACY_URL as string | undefined) ?? '/'
  return `${base.replace(/\/+$/, '')}/${path.replace(/^\/+/, '')}`
}
