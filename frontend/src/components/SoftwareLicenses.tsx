import { useQuery, useSuspenseQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { softwareLicensesQuery } from '../api/assets'
import { CREATE, hasRight, sessionQuery } from '../api/session'
import { ErrorState, Loading } from './States'

/** Licenses of a software, with a link to add one, like its license tab. */
export function SoftwareLicenses({ softwareId, softwareName }: { softwareId: number; softwareName: string }) {
  const { t } = useTranslation()
  const { data: session } = useSuspenseQuery(sessionQuery)
  const [trash, setTrash] = useState(false)
  const query = useQuery(softwareLicensesQuery(softwareId, trash))
  const link = 'font-medium text-indigo-600 hover:underline dark:text-indigo-400'

  return (
    <section
      aria-labelledby="licenses-title"
      className="space-y-3 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900"
    >
      <div className="flex flex-wrap items-center gap-3">
        <h2 id="licenses-title" className="text-sm font-semibold uppercase tracking-wide text-slate-500">
          {t('asset.license.title')}
        </h2>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={trash} onChange={(event) => setTrash(event.target.checked)} />
          {t('asset.license.showTrash')}
        </label>
        {hasRight(session, 'license', CREATE) && (
          <Link
            to="/assets/$kind/new"
            params={{ kind: 'software-licenses' }}
            search={{ software: softwareId, software_name: softwareName }}
            className="ml-auto rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
          >
            {t('asset.license.add')}
          </Link>
        )}
      </div>
      {query.isPending ? (
        <Loading />
      ) : query.isError ? (
        <ErrorState onRetry={() => void query.refetch()} />
      ) : query.data.items.length === 0 ? (
        <p className="py-4 text-center text-sm text-slate-500">{t('asset.license.empty')}</p>
      ) : (
        <ul className="divide-y divide-slate-100 text-sm dark:divide-slate-800">
          {query.data.items.map((license) => (
            <li key={license.id} className="flex flex-wrap items-baseline gap-x-3 py-2">
              <Link to="/assets/$kind/$id" params={{ kind: 'software-licenses', id: license.id }} className={link}>
                {license.name || t('assets.unnamed')}
              </Link>
              <span className="text-slate-500">{license.entity.name}</span>
              {license.type && <span className="text-slate-500">{license.type.name}</span>}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
