import { describe, expect, it } from 'vitest'
import { ApiError, toApiError } from '../api/client'
import { passwordErrorKeys } from './passwordErrors'

describe('passwordErrorKeys', () => {
  it('lists the broken rules of a weak password', () => {
    const error = toApiError(400, {
      error: { code: 'weak_password', message: '', details: ['too_short', 'missing_symbol', 'other'] },
    })
    expect(passwordErrorKeys(error)).toEqual(['too_short', 'missing_symbol'])
  })

  it('maps the other errors', () => {
    expect(passwordErrorKeys(new ApiError(400, 'wrong_password', ''))).toEqual(['wrong_password'])
    expect(passwordErrorKeys(new ApiError(429, 'too_many_attempts', ''))).toEqual(['too_many_attempts'])
    expect(passwordErrorKeys(new ApiError(500, 'internal', ''))).toEqual(['unknown'])
    expect(passwordErrorKeys(new Error('network'))).toEqual(['unknown'])
  })
})
