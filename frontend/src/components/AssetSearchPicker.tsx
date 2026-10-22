import { useQuery } from '@tanstack/react-query'
import { useEffect, useId, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { assetsQuery, type AssetType, type Choice } from '../api/assets'
import { usePopover } from '../hooks/usePopover'
import { Option } from './ChoicePicker'

const SEARCH_DELAY_MS = 250

/**
 * Picks an item of a type visible to the session, searching its name as
 * the user types (the first page of its list).
 */
export function AssetSearchPicker({
  kind,
  label,
  value,
  onChange,
}: {
  kind: AssetType
  label: string
  value: Choice | null
  onChange: (value: Choice | null) => void
}) {
  const { t } = useTranslation()
  const { open, setOpen, ref } = usePopover<HTMLDivElement>()
  const [text, setText] = useState('')
  const [search, setSearch] = useState('')
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  useEffect(() => () => clearTimeout(timer.current), [])
  const items = useQuery({
    ...assetsQuery(kind, { page: 1, search, sort: 'name', order: 'asc', trash: false }),
    enabled: open,
  })
  const labelId = useId()

  const onSearch = (next: string) => {
    setText(next)
    clearTimeout(timer.current)
    timer.current = setTimeout(() => setSearch(next.trim()), SEARCH_DELAY_MS)
  }
  const choose = (choice: Choice | null) => {
    onChange(choice)
    setOpen(false)
  }

  return (
    <div ref={ref} className="relative text-sm">
      <span id={labelId} className="block font-medium">
        {label}
      </span>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-labelledby={labelId}
        className="mt-1 flex w-full items-center justify-between rounded-md border border-slate-300 bg-white px-3 py-2 text-left shadow-sm hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-900 dark:hover:bg-slate-800"
      >
        <span className={`truncate ${value ? '' : 'text-slate-400'}`}>{value?.label ?? t('asset.form.none')}</span>
        <span aria-hidden="true" className="ml-2 text-slate-400">
          ▾
        </span>
      </button>
      {open && (
        <div className="absolute z-20 mt-1 w-full min-w-64 rounded-md border border-slate-200 bg-white p-2 shadow-lg dark:border-slate-700 dark:bg-slate-900">
          <input
            type="search"
            autoFocus
            value={text}
            onChange={(event) => onSearch(event.target.value)}
            placeholder={t('asset.form.search')}
            aria-label={`${label} : ${t('asset.form.search')}`}
            className="mb-2 w-full rounded-md border border-slate-300 bg-white px-2 py-1.5 dark:border-slate-700 dark:bg-slate-900"
          />
          <ul role="listbox" aria-labelledby={labelId} className="max-h-64 overflow-y-auto">
            {items.data?.items.map((item) => (
              <Option
                key={item.id}
                selected={value?.id === item.id}
                onSelect={() => choose({ id: item.id, label: item.name || `#${item.id}` })}
              >
                {item.name || `#${item.id}`}
                <span className="ml-2 text-xs text-slate-500">{item.entity.name}</span>
              </Option>
            ))}
          </ul>
          {items.isPending ? (
            <p className="px-2 py-1 text-slate-500">{t('app.loading')}</p>
          ) : items.isError ? (
            <p role="alert" className="px-2 py-1 text-red-700 dark:text-red-300">
              {t('errors.generic')}
            </p>
          ) : (
            items.data.items.length === 0 && <p className="px-2 py-1 text-slate-500">{t('asset.form.noChoice')}</p>
          )}
        </div>
      )}
    </div>
  )
}
