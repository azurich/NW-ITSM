import { useQuery, useSuspenseQuery } from '@tanstack/react-query'
import { Link, useNavigate, useParams, useSearch } from '@tanstack/react-router'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import {
  ASSET_TYPES,
  assetsQuery,
  type AssetListParams,
  type AssetPage as AssetPageData,
  type AssetSort,
  type AssetType,
} from '../api/assets'
import { CREATE, hasRight, sessionQuery } from '../api/session'
import { ErrorState, Loading } from '../components/States'
import { formatDateTime } from '../lib/format'
import { withDefaults, type AssetListSearch } from '../lib/search'

const COLUMNS: AssetSort[] = [
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

export function AssetsPage() {
  const { kind } = useParams({ from: '/authenticated/assets/$kind/' })
  // A new list per type: the search box starts empty.
  return <AssetList key={kind} kind={kind} />
}

function AssetList({ kind }: { kind: AssetType }) {
  const { t } = useTranslation()
  const { data: session } = useSuspenseQuery(sessionQuery)
  const search = useSearch({ from: '/authenticated/assets/$kind/' })
  const params = withDefaults(search)
  const navigate = useNavigate({ from: '/assets/$kind/' })
  const query = useQuery(assetsQuery(kind, params))

  const update = (changes: Partial<AssetListSearch>) =>
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

  const toggleSort = (column: AssetSort) =>
    update({
      sort: column === 'name' ? undefined : column,
      order: params.sort === column && params.order === 'asc' ? 'desc' : undefined,
      page: undefined,
    })

  if (query.error instanceof ApiError && query.error.status === 403) {
    return <ErrorState message={t('assets.forbidden')} />
  }

  const title = t(`assets.types.${kind}`)
  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-2xl font-semibold">
          {params.trash ? `${title} — ${t('assets.trash')}` : title}
        </h1>
        <button
          type="button"
          onClick={() => void update({ trash: params.trash ? undefined : true, page: undefined })}
          aria-pressed={params.trash}
          className="rounded-md border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
        >
          {params.trash ? t('assets.hideTrash') : t('assets.showTrash')}
        </button>
        {hasRight(session, ASSET_TYPES[kind].right, CREATE) && !params.trash && (
          <>
            <Link
              to="/assets/$kind/new"
              params={{ kind }}
              className="rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
            >
              {t('assets.new')}
            </Link>
            <Link
              to="/assets/$kind/templates"
              params={{ kind }}
              className="rounded-md border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
            >
              {t('templates.manage')}
            </Link>
          </>
        )}
        <input
          type="search"
          value={text}
          onChange={(event) => onSearch(event.target.value)}
          placeholder={ASSET_TYPES[kind].uuid ? t('assets.searchWithUuid') : t('assets.search')}
          aria-label={t('assets.search')}
          className="ml-auto w-full max-w-md rounded-md border border-slate-300 bg-white px-3 py-2 text-sm shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/30 dark:border-slate-700 dark:bg-slate-900"
        />
      </div>

      {query.isPending ? (
        <Loading />
      ) : query.isError ? (
        <ErrorState onRetry={() => void query.refetch()} />
      ) : (
        <Results
          kind={kind}
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
  kind: AssetType
  page: AssetPageData
  params: AssetListParams
  dimmed: boolean
  onSort: (column: AssetSort) => void
  onPage: (page: number) => void
}

function Results({ kind, page, params, dimmed, onSort, onPage }: ResultsProps) {
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
                    {t(`assets.columns.${column}`)}
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
                  {params.trash ? t('assets.emptyTrash') : t('assets.empty')}
                </td>
              </tr>
            ) : (
              page.items.map((asset) => (
                <tr key={asset.id} className="hover:bg-slate-50 dark:hover:bg-slate-800/50">
                  <td className="whitespace-nowrap px-3 py-2">
                    <Link
                      to="/assets/$kind/$id"
                      params={{ kind, id: asset.id }}
                      className="font-medium text-indigo-600 hover:underline dark:text-indigo-400"
                    >
                      {asset.name || t('assets.unnamed')}
                    </Link>
                  </td>
                  <td className="px-3 py-2">{asset.entity.name}</td>
                  <td className="px-3 py-2">{asset.serial}</td>
                  <td className="px-3 py-2">{asset.inventory_number}</td>
                  <td className="px-3 py-2">{asset.type?.name}</td>
                  <td className="px-3 py-2">{asset.manufacturer?.name}</td>
                  <td className="px-3 py-2">{asset.state?.name}</td>
                  <td className="px-3 py-2">{asset.location?.name}</td>
                  <td className="whitespace-nowrap px-3 py-2">
                    {formatDateTime(asset.updated_at, i18n.language)}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <nav className="flex items-center justify-between text-sm" aria-label="pagination">
        <span className="text-slate-500">{t('assets.range', { from, to, total: page.total })}</span>
        <span className="flex gap-2">
          <button
            type="button"
            disabled={params.page <= 1}
            onClick={() => onPage(params.page - 1)}
            className="rounded-md border border-slate-300 px-3 py-1.5 enabled:hover:bg-slate-100 disabled:opacity-40 dark:border-slate-700 dark:enabled:hover:bg-slate-800"
          >
            {t('assets.previous')}
          </button>
          <button
            type="button"
            disabled={params.page >= lastPage}
            onClick={() => onPage(params.page + 1)}
            className="rounded-md border border-slate-300 px-3 py-1.5 enabled:hover:bg-slate-100 disabled:opacity-40 dark:border-slate-700 dark:enabled:hover:bg-slate-800"
          >
            {t('assets.next')}
          </button>
        </span>
      </nav>
    </>
  )
}
