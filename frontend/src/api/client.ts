import createClient from 'openapi-fetch'
import type { components, paths } from './schema'

/** Types of the /api/v2 contract, generated from backend/openapi.json. */
export type Schemas = components['schemas']

export const api = createClient<paths>({ credentials: 'same-origin' })

/** Error returned by the API: `{"error": {"code", "message"}}`. */
export class ApiError extends Error {
  readonly status: number
  readonly code: string
  /** More codes, e.g. the broken rules of `weak_password`. */
  readonly details: string[]

  constructor(status: number, code: string, message: string, details: string[] = []) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
    this.details = details
  }
}

export function toApiError(status: number, body: unknown): ApiError {
  const detail = (
    body as { error?: { code?: unknown; message?: unknown; details?: unknown } } | null | undefined
  )?.error
  const code = typeof detail?.code === 'string' ? detail.code : status >= 500 ? 'internal' : 'unknown'
  const message = typeof detail?.message === 'string' ? detail.message : `HTTP ${status}`
  const details = Array.isArray(detail?.details)
    ? detail.details.filter((item): item is string => typeof item === 'string')
    : []
  return new ApiError(status, code, message, details)
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
