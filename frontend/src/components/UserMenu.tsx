import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { logout, type SessionInfo } from '../api/session'
import { usePopover } from '../hooks/usePopover'
import { LegacyLink } from './LegacyLink'

const itemClass =
  'block w-full rounded px-3 py-1.5 text-left hover:bg-slate-100 dark:hover:bg-slate-800'

export function UserMenu({ session }: { session: SessionInfo }) {
  const { t } = useTranslation()
  const { open, setOpen, ref } = usePopover<HTMLDivElement>()
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const logoutMutation = useMutation({
    mutationFn: logout,
    onSettled: () => {
      queryClient.clear()
      void navigate({ to: '/login', search: {} })
    },
  })

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={t('app.userMenu')}
        className="rounded-md px-2 py-1 text-right leading-tight hover:bg-slate-100 dark:hover:bg-slate-800"
      >
        <span className="block font-medium">{session.user.display_name}</span>
        <span className="block text-xs text-slate-500">{session.profile.name}</span>
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 z-20 mt-2 w-56 rounded-lg border border-slate-200 bg-white p-1 shadow-lg dark:border-slate-700 dark:bg-slate-900"
        >
          <Link to="/preferences" role="menuitem" onClick={() => setOpen(false)} className={itemClass}>
            {t('app.preferences')}
          </Link>
          <LegacyLink path="front/central.php" className={itemClass}>
            {t('app.legacy')}
          </LegacyLink>
          <hr className="my-1 border-slate-200 dark:border-slate-700" />
          <button
            type="button"
            role="menuitem"
            onClick={() => logoutMutation.mutate()}
            disabled={logoutMutation.isPending}
            className={itemClass}
          >
            {t('app.logout')}
          </button>
        </div>
      )}
    </div>
  )
}
