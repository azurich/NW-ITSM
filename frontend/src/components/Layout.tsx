import { useMutation, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import { Link, Outlet, useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { READ, hasRight, logout, sessionQuery } from '../api/session'
import { legacyUrl } from '../lib/format'

export function Layout() {
  const { t } = useTranslation()
  const { data: session } = useSuspenseQuery(sessionQuery)
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
    <div className="min-h-screen">
      <header className="border-b border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center gap-x-6 gap-y-2 px-4 py-3">
          <Link to="/" className="text-lg font-semibold text-indigo-600 dark:text-indigo-400">
            {t('app.name')}
          </Link>
          <nav className="flex items-center gap-4 text-sm" aria-label={t('nav.assets')}>
            {hasRight(session, 'computer', READ) && (
              <Link
                to="/assets/computers"
                className="text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
                activeProps={{ className: 'font-medium text-slate-900 dark:text-white' }}
              >
                {t('nav.computers')}
              </Link>
            )}
          </nav>
          <div className="ml-auto flex items-center gap-4 text-sm">
            <a href={legacyUrl('/')} className="text-slate-500 hover:underline">
              {t('app.legacy')}
            </a>
            <span className="text-right leading-tight">
              <span className="block font-medium">{session.user.display_name}</span>
              <span className="block text-xs text-slate-500">{session.profile.name}</span>
            </span>
            <button
              type="button"
              onClick={() => logoutMutation.mutate()}
              disabled={logoutMutation.isPending}
              className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 disabled:opacity-50 dark:border-slate-700 dark:hover:bg-slate-800"
            >
              {t('app.logout')}
            </button>
          </div>
        </div>
      </header>
      <main className="mx-auto max-w-7xl px-4 py-6">
        <Outlet />
      </main>
    </div>
  )
}
