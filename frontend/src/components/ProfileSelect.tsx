import { useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { switchProfile, type SessionInfo } from '../api/session'
import { useApplySession } from '../hooks/useApplySession'

/** Switches between the profiles of the user (hidden with only one). */
export function ProfileSelect({ session }: { session: SessionInfo }) {
  const { t } = useTranslation()
  const applySession = useApplySession()
  const mutation = useMutation({
    mutationFn: switchProfile,
    onSuccess: applySession,
  })

  if (session.profiles.length < 2) {
    return null
  }
  return (
    <select
      value={session.profile.id}
      onChange={(event) => mutation.mutate(Number(event.target.value))}
      disabled={mutation.isPending}
      aria-label={t('header.profile')}
      className="rounded-md border border-slate-300 bg-white px-2 py-1.5 dark:border-slate-700 dark:bg-slate-900"
    >
      {session.profiles.map((profile) => (
        <option key={profile.id} value={profile.id}>
          {profile.name}
        </option>
      ))}
    </select>
  )
}
