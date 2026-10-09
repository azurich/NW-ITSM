import { describe, expect, it } from 'vitest'
import { ApiError, isUnauthenticated, toApiError, unwrap } from './client'

const response = (status: number) => new Response(null, { status })

describe('unwrap', () => {
  it('returns the payload of a successful call', () => {
    expect(unwrap({ data: { id: 1 }, response: response(200) })).toEqual({ id: 1 })
  })

  it('throws the API error', () => {
    const error = { error: { code: 'not_found', message: 'not found' } }
    expect(() => unwrap({ error, response: response(404) })).toThrow(new ApiError(404, 'not_found', 'not found'))
  })
})

describe('toApiError', () => {
  it('tolerates bodies that are not API errors', () => {
    expect(toApiError(502, 'Bad Gateway')).toMatchObject({ status: 502, code: 'internal', message: 'HTTP 502' })
    expect(toApiError(400, null)).toMatchObject({ code: 'unknown' })
  })

  it('detects an expired session', () => {
    expect(isUnauthenticated(toApiError(401, undefined))).toBe(true)
    expect(isUnauthenticated(toApiError(403, undefined))).toBe(false)
    expect(isUnauthenticated(new Error('network'))).toBe(false)
  })
})
