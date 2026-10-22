import { describe, expect, it } from 'vitest'
import { toLanguage } from './language'

describe('toLanguage', () => {
  it('maps ITSM-NG and browser locales', () => {
    expect(toLanguage('fr_FR')).toBe('fr')
    expect(toLanguage('en-GB')).toBe('en')
    expect(toLanguage('de_DE')).toBeUndefined()
    expect(toLanguage(null)).toBeUndefined()
  })
})
