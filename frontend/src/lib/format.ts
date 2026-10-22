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

/**
 * Formats an API date (`YYYY-MM-DD`, no time zone) for display, or returns
 * `fallback`: the day stays the same whatever the time zone of the browser.
 */
export function formatDate(value: string | null | undefined, locale: string, fallback = '—'): string {
  const parts = value?.match(/^(\d{4})-(\d{2})-(\d{2})$/)
  if (!parts) {
    return fallback
  }
  const date = new Date(Number(parts[1]), Number(parts[2]) - 1, Number(parts[3]))
  return new Intl.DateTimeFormat(locale, { dateStyle: 'short' }).format(date)
}

/** URL of a page of the legacy PHP application. */
export function legacyUrl(path: string): string {
  const base = (import.meta.env.VITE_LEGACY_URL as string | undefined) ?? '/'
  return `${base.replace(/\/+$/, '')}/${path.replace(/^\/+/, '')}`
}
