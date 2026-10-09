import createClient from 'openapi-fetch'
import type { components, paths } from './schema'

/** Types of the /api/v2 contract, generated from backend/openapi.json. */
export type Schemas = components['schemas']

export const api = createClient<paths>({ credentials: 'same-origin' })

/** Error returned by the API: `{"error": {"code", "message"}}`. */
export class ApiError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
  }
}

export function toApiError(status: number, body: unknown): ApiError {
  const detail = (body as { error?: { code?: unknown; message?: unknown } } | null | undefined)?.error
  const code = typeof detail?.code === 'string' ? detail.code : status >= 500 ? 'internal' : 'unknown'
  const message = typeof detail?.message === 'string' ? detail.message : `HTTP ${status}`
  return new ApiError(status, code, message)
}

/** Payload of a successful call; throws an {@link ApiError} otherwise. */
export function unwrap<T>(result: { data?: T; error?: unknown; response: Response }): T {
  if (result.response.ok && result.data !== undefined) {
    return result.data
  }
  throw toApiError(result.response.status, result.error)
}

export function isUnauthenticated(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401
}
