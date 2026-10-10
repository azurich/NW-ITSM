import { useQuery } from '@tanstack/react-query'
import { useEffect, useId, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { choicesQuery, type AssetType, type Choice, type ChoiceField } from '../api/assets'
import { usePopover } from '../hooks/usePopover'

const SEARCH_DELAY_MS = 250

/**
 * Picks a value of a form field among those offered for the entity,
 * searching as the user types.
 */
export function ChoicePicker({
  kind,
  field,
  entityId,
  label,
  value,
  onChange,
  disabled,
}: {
  kind: AssetType
  field: ChoiceField
  entityId: number
  label: string
  value: Choice | null
  onChange: (value: Choice | null) => void
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const { open, setOpen, ref } = usePopover<HTMLDivElement>()
  const [text, setText] = useState('')
  const [search, setSearch] = useState('')
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  useEffect(() => () => clearTimeout(timer.current), [])
  const choices = useQuery({ ...choicesQuery(kind, field, entityId, search), enabled: open })
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
        disabled={disabled}
        onClick={() => setOpen(!open)}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-labelledby={labelId}
        className="mt-1 flex w-full items-center justify-between rounded-md border border-slate-300 bg-white px-3 py-2 text-left shadow-sm hover:bg-slate-50 disabled:opacity-60 dark:border-slate-700 dark:bg-slate-900 dark:hover:bg-slate-800"
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
            <Option selected={value === null} onSelect={() => choose(null)}>
              <span className="text-slate-500">{t('asset.form.none')}</span>
            </Option>
            {choices.data?.map((choice) => (
              <Option key={choice.id} selected={value?.id === choice.id} onSelect={() => choose(choice)}>
                {choice.label}
              </Option>
            ))}
          </ul>
          {choices.isPending ? (
            <p className="px-2 py-1 text-slate-500">{t('app.loading')}</p>
          ) : choices.isError ? (
            <p role="alert" className="px-2 py-1 text-red-700 dark:text-red-300">
              {t('errors.generic')}
            </p>
          ) : (
            choices.data.length === 0 && <p className="px-2 py-1 text-slate-500">{t('asset.form.noChoice')}</p>
          )}
        </div>
      )}
    </div>
  )
}

function Option({
  selected,
  onSelect,
  children,
}: {
  selected: boolean
  onSelect: () => void
  children: ReactNode
}) {
  return (
    <li role="option" aria-selected={selected}>
      <button
        type="button"
        onClick={onSelect}
        className={`w-full rounded px-2 py-1.5 text-left hover:bg-slate-100 focus:bg-slate-100 focus:outline-none dark:hover:bg-slate-800 dark:focus:bg-slate-800 ${
          selected ? 'font-medium text-indigo-700 dark:text-indigo-300' : ''
        }`}
      >
        {children}
      </button>
    </li>
  )
}
