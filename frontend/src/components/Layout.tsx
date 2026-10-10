import { useSuspenseQuery } from '@tanstack/react-query'
import { Link, Outlet } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { READ, hasRight, sessionQuery } from '../api/session'
import { EntitySelector } from './EntitySelector'
import { ProfileSelect } from './ProfileSelect'
import { UserMenu } from './UserMenu'

export function Layout() {
  const { t } = useTranslation()
  const { data: session } = useSuspenseQuery(sessionQuery)

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
          <div className="ml-auto flex flex-wrap items-center gap-3 text-sm">
            <EntitySelector session={session} />
            <ProfileSelect session={session} />
            <UserMenu session={session} />
          </div>
        </div>
      </header>
      <main className="mx-auto max-w-7xl px-4 py-6">
        <Outlet />
      </main>
    </div>
  )
}
