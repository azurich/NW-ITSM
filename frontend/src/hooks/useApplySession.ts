import { useQueryClient } from '@tanstack/react-query'
import { sessionQuery, type SessionInfo } from '../api/session'
import { applyUserLanguage } from '../i18n'

/**
 * Stores a session returned by the API (new profile, entities or
 * language) and reloads the data that depends on it.
 */
export function useApplySession() {
  const queryClient = useQueryClient()
  return async (session: SessionInfo) => {
    queryClient.setQueryData(sessionQuery.queryKey, session)
    applyUserLanguage(session.user.language)
    await queryClient.resetQueries({
      predicate: (query) => !['session', 'languages'].includes(String(query.queryKey[0])),
    })
  }
}
