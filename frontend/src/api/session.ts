import { queryOptions } from '@tanstack/react-query'
import { api, toApiError, unwrap, type Schemas } from './client'

export type SessionInfo = Schemas['SessionInfo']

/** Legacy right bits (inc/define.php). */
export const READ = 1

export const sessionQuery = queryOptions({
  queryKey: ['session'],
  queryFn: async () => unwrap(await api.GET('/api/v2/auth/session')),
  staleTime: 60_000,
  retry: false,
})

export async function login(login: string, password: string): Promise<SessionInfo> {
  return unwrap(await api.POST('/api/v2/auth/login', { body: { login, password } }))
}

export async function logout(): Promise<void> {
  const { response } = await api.POST('/api/v2/auth/logout')
  if (!response.ok) {
    throw toApiError(response.status, undefined)
  }
}

/** Same as the legacy `Session::haveRight`: at least one bit is set. */
export function hasRight(session: SessionInfo, module: string, right: number): boolean {
  return ((session.rights[module] ?? 0) & right) !== 0
}
