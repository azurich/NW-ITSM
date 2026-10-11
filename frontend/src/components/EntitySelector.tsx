import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { selectableEntitiesQuery, switchEntity, type SessionInfo } from '../api/session'
import { useApplySession } from '../hooks/useApplySession'
import { usePopover } from '../hooks/usePopover'

/** Shows more than this many entities: a filter box appears. */
const FILTER_THRESHOLD = 8

/** Selects the entities to work on, like the legacy entity selector. */
export function EntitySelector({ session }: { session: SessionInfo }) {
  const { t } = useTranslation()
  const { open, setOpen, ref } = usePopover<HTMLDivElement>()
  const [filter, setFilter] = useState('')
  const [recursive, setRecursive] = useState(session.entities.recursive)
  const entities = useQuery({ ...selectableEntitiesQuery, enabled: open })
  const applySession = useApplySession()
  const mutation = useMutation({
    mutationFn: ({ id, recursive }: { id: number | null; recursive: boolean }) => switchEntity(id, recursive),
    onSuccess: async (updated) => {
      setOpen(false)
      await applySession(updated)
    },
  })

  const selected = session.entities.selected
  const label = selected
    ? `${selected.name ?? `#${selected.id}`}${session.entities.recursive ? ` (${t('header.withSubEntities')})` : ''}`
    : t('header.allEntities')
  const items = (entities.data ?? []).filter((entity) =>
    (entity.completename ?? entity.name ?? '').toLowerCase().includes(filter.trim().toLowerCase()),
  )
  const minLevel = Math.min(...(entities.data ?? []).map((entity) => entity.level))

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        aria-haspopup="dialog"
        title={t('header.entity')}
        className="max-w-xs truncate rounded-md border border-slate-300 px-3 py-1.5 text-left hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
      >
        <span className="sr-only">{t('header.entity')} : </span>
        {label}
      </button>
      {open && (
        <div
          role="dialog"
          aria-label={t('header.entity')}
          className="absolute right-0 z-20 mt-2 w-80 rounded-lg border border-slate-200 bg-white p-3 shadow-lg dark:border-slate-700 dark:bg-slate-900"
        >
          {(entities.data?.length ?? 0) > FILTER_THRESHOLD && (
            <input
              type="search"
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              placeholder={t('header.filterEntities')}
              aria-label={t('header.filterEntities')}
              className="mb-2 w-full rounded-md border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-950"
            />
          )}
          <label className="mb-2 flex items-center gap-2 text-sm">
            <input type="checkbox" checked={recursive} onChange={(event) => setRecursive(event.target.checked)} />
            {t('header.includeSubEntities')}
          </label>
          <ul className="max-h-72 overflow-y-auto text-sm">
            <li>
              <button
                type="button"
                disabled={mutation.isPending}
                onClick={() => mutation.mutate({ id: null, recursive: false })}
                className="w-full rounded px-2 py-1 text-left font-medium hover:bg-slate-100 dark:hover:bg-slate-800"
              >
                {t('header.allEntities')}
              </button>
            </li>
            {entities.isPending ? (
              <li className="px-2 py-1 text-slate-500">{t('app.loading')}</li>
            ) : items.length === 0 ? (
              <li className="px-2 py-1 text-slate-500">{t('header.noEntity')}</li>
            ) : (
              items.map((entity) => (
                <li key={entity.id}>
                  <button
                    type="button"
                    disabled={mutation.isPending}
                    onClick={() =>
                      mutation.mutate({ id: entity.id, recursive: recursive && entity.recursive_allowed })
                    }
                    title={entity.completename ?? undefined}
                    style={{ paddingLeft: `${0.5 + (entity.level - minLevel) * 1}rem` }}
                    className={`w-full truncate rounded py-1 pr-2 text-left hover:bg-slate-100 dark:hover:bg-slate-800 ${
                      selected?.id === entity.id ? 'font-semibold text-indigo-600 dark:text-indigo-400' : ''
                    }`}
                  >
                    {entity.name ?? `#${entity.id}`}
                  </button>
                </li>
              ))
            )}
          </ul>
        </div>
      )}
    </div>
  )
}
