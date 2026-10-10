import { queryOptions } from '@tanstack/react-query'
import { api, toApiError, unwrap, type Schemas } from './client'

export type SessionInfo = Schemas['SessionInfo']
export type SelectableEntity = Schemas['SelectableEntity']
export type Language = Schemas['Language']

/** Legacy right bits (inc/define.php). */
export const READ = 1
export const UPDATE = 2
export const CREATE = 4
export const DELETE = 8

export const sessionQuery = queryOptions({
  queryKey: ['session'],
  queryFn: async () => unwrap(await api.GET('/api/v2/auth/session')),
  staleTime: 60_000,
  retry: false,
})

export const selectableEntitiesQuery = queryOptions({
  queryKey: ['selectable-entities'],
  queryFn: async () => unwrap(await api.GET('/api/v2/auth/session/entities')).items,
  staleTime: 5 * 60_000,
})

export const languagesQuery = queryOptions({
  queryKey: ['languages'],
  queryFn: async () => unwrap(await api.GET('/api/v2/languages')),
  staleTime: Infinity,
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

/** Replaces an expired password, then logs in. */
export async function replaceExpiredPassword(
  login: string,
  currentPassword: string,
  newPassword: string,
): Promise<SessionInfo> {
  return unwrap(
    await api.POST('/api/v2/auth/password/expired', {
      body: { login, current_password: currentPassword, new_password: newPassword },
    }),
  )
}

export async function changePassword(currentPassword: string, newPassword: string): Promise<SessionInfo> {
  return unwrap(
    await api.POST('/api/v2/auth/password', {
      body: { current_password: currentPassword, new_password: newPassword },
    }),
  )
}

export async function switchProfile(profileId: number): Promise<SessionInfo> {
  return unwrap(await api.PUT('/api/v2/auth/session/profile', { body: { profile_id: profileId } }))
}

/** `null` selects every entity of the profile. */
export async function switchEntity(entityId: number | null, recursive: boolean): Promise<SessionInfo> {
  return unwrap(
    await api.PUT('/api/v2/auth/session/entity', { body: { entity_id: entityId, recursive } }),
  )
}

/** `null` goes back to the default language. */
export async function updateLanguage(language: string | null): Promise<SessionInfo> {
  return unwrap(await api.PATCH('/api/v2/me/preferences', { body: { language } }))
}

/** One-time token opening the classic interface as the current user. */
export async function bridgeToken(): Promise<string> {
  return unwrap(await api.POST('/api/v2/auth/bridge/legacy')).token
}

/** Same as the legacy `Session::haveRight`: at least one bit is set. */
export function hasRight(session: SessionInfo, module: string, right: number): boolean {
  return ((session.rights[module] ?? 0) & right) !== 0
}
