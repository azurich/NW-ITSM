import { describe, expect, it } from 'vitest'
import { formatDateTime, legacyUrl } from './format'

describe('formatDateTime', () => {
  it('uses the fallback for missing or invalid dates', () => {
    expect(formatDateTime(null, 'fr')).toBe('—')
    expect(formatDateTime('not a date', 'fr', '?')).toBe('?')
  })

  it('formats with the locale', () => {
    expect(formatDateTime('2026-01-31T10:00:00Z', 'fr')).toMatch(/31\/01\/2026/)
  })
})

describe('legacyUrl', () => {
  it('joins the legacy base and the page', () => {
    expect(legacyUrl('front/computer.form.php?id=2')).toBe('/front/computer.form.php?id=2')
    expect(legacyUrl('/')).toBe('/')
  })
})
