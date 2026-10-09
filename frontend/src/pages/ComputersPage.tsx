import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import {
  computersQuery,
  type ComputerListParams,
  type ComputerPage as ComputerPageData,
  type ComputerSort,
} from '../api/computers'
import { ErrorState, Loading } from '../components/States'
import { formatDateTime } from '../lib/format'
import { withDefaults, type ComputerListSearch } from '../lib/search'

const COLUMNS: ComputerSort[] = [
  'name',
  'entity',
  'serial',
  'inventory_number',
  'type',
  'manufacturer',
  'state',
  'location',
  'updated_at',
]

const SEARCH_DELAY_MS = 300

export function ComputersPage() {
  const { t } = useTranslation()
  const search = useSearch({ from: '/authenticated/assets/computers' })
  const params = withDefaults(search)
  const navigate = useNavigate({ from: '/assets/computers' })
  const query = useQuery(computersQuery(params))

  const update = (changes: Partial<ComputerListSearch>) =>
    navigate({ search: (previous) => ({ ...previous, ...changes }), replace: true })

  // The URL follows the search box once the user stops typing, and the
  // box follows the URL when it changes otherwise (links, history).
  const [text, setText] = useState(params.search)
  const [syncedSearch, setSyncedSearch] = useState(params.search)
  if (params.search !== syncedSearch) {
    setSyncedSearch(params.search)
    if (params.search !== text.trim()) {
      setText(params.search)
    }
  }
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  useEffect(() => () => clearTimeout(timer.current), [])
  const onSearch = (value: string) => {
    setText(value)
    clearTimeout(timer.current)
    timer.current = setTimeout(
      () => void update({ search: value.trim() || undefined, page: undefined }),
      SEARCH_DELAY_MS,
    )
  }

  const toggleSort = (column: ComputerSort) =>
    update({
      sort: column === 'name' ? undefined : column,
      order: params.sort === column && params.order === 'asc' ? 'desc' : undefined,
      page: undefined,
    })

  if (query.error instanceof ApiError && query.error.status === 403) {
    return <ErrorState message={t('computers.forbidden')} />
  }

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold">{t('computers.title')}</h1>
        <input
          type="search"
          value={text}
          onChange={(event) => onSearch(event.target.value)}
          placeholder={t('computers.search')}
          aria-label={t('computers.search')}
          className="w-full max-w-md rounded-md border border-slate-300 bg-white px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/30 dark:border-slate-700 dark:bg-slate-900"
        />
      </div>

      {query.isPending ? (
        <Loading />
      ) : query.isError ? (
        <ErrorState onRetry={() => void query.refetch()} />
      ) : (
        <Results
          page={query.data}
          params={params}
          dimmed={query.isPlaceholderData}
          onSort={toggleSort}
          onPage={(target) => void update({ page: target > 1 ? target : undefined })}
        />
      )}
    </section>
  )
}

interface ResultsProps {
  page: ComputerPageData
  params: ComputerListParams
  dimmed: boolean
  onSort: (column: ComputerSort) => void
  onPage: (page: number) => void
}

function Results({ page, params, dimmed, onSort, onPage }: ResultsProps) {
  const { t, i18n } = useTranslation()
  const from = page.total > 0 ? (page.page - 1) * page.per_page + 1 : 0
  const to = Math.min(page.page * page.per_page, page.total)
  const lastPage = Math.max(1, Math.ceil(page.total / page.per_page))

  return (
    <>
      <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900">
        <table className={`min-w-full text-left text-sm ${dimmed ? 'opacity-60' : ''}`}>
          <thead className="border-b border-slate-200 bg-slate-50 text-xs uppercase text-slate-500 dark:border-slate-800 dark:bg-slate-900/60">
            <tr>
              {COLUMNS.map((column) => (
                <th
                  key={column}
                  scope="col"
                  className="whitespace-nowrap px-3 py-2 font-medium"
                  aria-sort={
                    params.sort === column ? (params.order === 'asc' ? 'ascending' : 'descending') : undefined
                  }
                >
                  <button type="button" onClick={() => onSort(column)} className="hover:text-slate-900 dark:hover:text-white">
                    {t(`computers.columns.${column}`)}
                    {params.sort === column && (params.order === 'asc' ? ' ▲' : ' ▼')}
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100 dark:divide-slate-800">
            {page.items.length === 0 ? (
              <tr>
                <td colSpan={COLUMNS.length} className="px-3 py-8 text-center text-slate-500">
                  {t('computers.empty')}
                </td>
              </tr>
            ) : (
              page.items.map((computer) => (
                <tr key={computer.id} className="hover:bg-slate-50 dark:hover:bg-slate-800/50">
                  <td className="whitespace-nowrap px-3 py-2">
                    <Link
                      to="/assets/computers/$computerId"
                      params={{ computerId: computer.id }}
                      className="font-medium text-indigo-600 hover:underline dark:text-indigo-400"
                    >
                      {computer.name || t('computers.unnamed')}
                    </Link>
                  </td>
                  <td className="px-3 py-2">{computer.entity.name}</td>
                  <td className="px-3 py-2">{computer.serial}</td>
                  <td className="px-3 py-2">{computer.inventory_number}</td>
                  <td className="px-3 py-2">{computer.type?.name}</td>
                  <td className="px-3 py-2">{computer.manufacturer?.name}</td>
                  <td className="px-3 py-2">{computer.state?.name}</td>
                  <td className="px-3 py-2">{computer.location?.name}</td>
                  <td className="whitespace-nowrap px-3 py-2">
                    {formatDateTime(computer.updated_at, i18n.language)}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <nav className="flex items-center justify-between text-sm" aria-label="pagination">
        <span className="text-slate-500">{t('computers.range', { from, to, total: page.total })}</span>
        <span className="flex gap-2">
          <button
            type="button"
            disabled={params.page <= 1}
            onClick={() => onPage(params.page - 1)}
            className="rounded-md border border-slate-300 px-3 py-1.5 enabled:hover:bg-slate-100 disabled:opacity-40 dark:border-slate-700 dark:enabled:hover:bg-slate-800"
          >
            {t('computers.previous')}
          </button>
          <button
            type="button"
            disabled={params.page >= lastPage}
            onClick={() => onPage(params.page + 1)}
            className="rounded-md border border-slate-300 px-3 py-1.5 enabled:hover:bg-slate-100 disabled:opacity-40 dark:border-slate-700 dark:enabled:hover:bg-slate-800"
          >
            {t('computers.next')}
          </button>
        </span>
      </nav>
    </>
  )
}
