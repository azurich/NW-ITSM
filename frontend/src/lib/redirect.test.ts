import { describe, expect, it } from 'vitest'
import { isSafeRedirect } from './redirect'

describe('isSafeRedirect', () => {
  it('accepts application paths', () => {
    expect(isSafeRedirect('/assets/computers?page=2')).toBe(true)
  })

  it('rejects other origins', () => {
    for (const value of ['https://evil.example', '//evil.example', '/\\evil.example', 'assets', '', 42, undefined]) {
      expect(isSafeRedirect(value)).toBe(false)
    }
  })
})
