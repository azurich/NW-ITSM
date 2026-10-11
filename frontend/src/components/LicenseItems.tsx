import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import {
  ASSET_TYPES,
  affectLicense,
  licenseItemsQuery,
  removeAffectation,
  type AssetType,
  type Choice,
  type LicenseItem,
} from '../api/assets'
import { WriteError } from './AssetForm'
import { AssetSearchPicker } from './AssetSearchPicker'
import { inputClass } from './PasswordFields'
import { ErrorState, Loading } from './States'

/** The types with software, which a license is affected to. */
export const SOFTWARE_TYPES = (Object.keys(ASSET_TYPES) as AssetType[]).filter(
  (kind) => ASSET_TYPES[kind].installations,
)

/**
 * Items a license is affected to, like its tab in the classic interface:
 * affected from an item of a type with software, removed for good, both
 * noted in the history of the item and of the license.
 */
export function LicenseItems({ licenseId, editable }: { licenseId: number; editable: boolean }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const query = useQuery(licenseItemsQuery(licenseId))
  const [kind, setKind] = useState<AssetType>('computers')
  const [item, setItem] = useState<Choice | null>(null)
  const [confirm, setConfirm] = useState<LicenseItem | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const legacyPath = `front/softwarelicense.form.php?id=${licenseId}`

  // The validity of the license and of its software may change.
  const refresh = () => queryClient.invalidateQueries({ predicate: affectedBy })
  const affect = useMutation({
    mutationFn: () => affectLicense(licenseId, kind, item?.id ?? 0),
    onSuccess: async () => {
      setItem(null)
      setNotice(t('asset.affectations.affected'))
      await refresh()
    },
  })
  const remove = useMutation({
    mutationFn: (row: LicenseItem) => removeAffectation(licenseId, row.id),
    onSuccess: async () => {
      setConfirm(null)
      setNotice(t('asset.affectations.removed'))
      await refresh()
    },
  })

  const onSubmit = (event: FormEvent) => {
    event.preventDefault()
    setNotice(null)
    remove.reset()
    affect.mutate()
  }
  const name = (row: LicenseItem) => row.item.name || `#${row.item.id}`
  const canEdit = editable && !!query.data?.can_edit

  return (
    <section
      aria-labelledby="license-items-title"
      className="space-y-3 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900"
    >
      <h2 id="license-items-title" className="text-sm font-semibold uppercase tracking-wide text-slate-500">
        {t('asset.affectations.itemsTitle')}
      </h2>
      {notice && (
        <p role="status" className="text-sm text-green-700 dark:text-green-400">
          {notice}
        </p>
      )}

      {canEdit && (
        <form
          onSubmit={onSubmit}
          aria-label={t('asset.affectations.affectItem')}
          className="grid items-end gap-4 sm:grid-cols-[1fr_1fr_auto]"
        >
          <label className="block text-sm font-medium">
            {t('asset.affectations.type')}
            <select
              value={kind}
              onChange={(event) => {
                setKind(event.target.value as AssetType)
                setItem(null)
              }}
              className={inputClass}
            >
              {SOFTWARE_TYPES.map((type) => (
                <option key={type} value={type}>
                  {t(`assets.types.${type}`)}
                </option>
              ))}
            </select>
          </label>
          <AssetSearchPicker
            key={kind}
            kind={kind}
            label={t('asset.affectations.item')}
            value={item}
            onChange={setItem}
          />
          <button
            type="submit"
            disabled={!item || affect.isPending}
            className="rounded-md bg-indigo-600 px-3 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-60"
          >
            {t('asset.affectations.affect')}
          </button>
        </form>
      )}
      {affect.isError && <WriteError error={affect.error} legacyPath={legacyPath} />}

      {confirm && (
        <RemoveConfirm
          name={name(confirm)}
          pending={remove.isPending}
          onConfirm={() => remove.mutate(confirm)}
          onCancel={() => {
            setConfirm(null)
            remove.reset()
          }}
        />
      )}
      {remove.isError && <WriteError error={remove.error} legacyPath={legacyPath} />}

      {query.isPending ? (
        <Loading />
      ) : query.isError ? (
        <ErrorState onRetry={() => void query.refetch()} />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="border-b border-slate-200 text-xs uppercase text-slate-500 dark:border-slate-800">
              <tr>
                {(['type', 'item', 'entity', 'serial', 'inventoryNumber'] as const).map((column) => (
                  <th key={column} scope="col" className="px-3 py-2 font-medium">
                    {t(`asset.affectations.${column}`)}
                  </th>
                ))}
                {canEdit && (
                  <th scope="col" className="px-3 py-2">
                    <span className="sr-only">{t('asset.affectations.actions')}</span>
                  </th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-slate-800">
              {query.data.items.length === 0 && (
                <tr>
                  <td colSpan={canEdit ? 6 : 5} className="px-3 py-6 text-center text-slate-500">
                    {t('asset.affectations.noItem')}
                  </td>
                </tr>
              )}
              {query.data.items.map((row) => (
                <tr key={row.id}>
                  <td className="px-3 py-2">{t(`assets.types.${row.kind}`)}</td>
                  <td className="whitespace-nowrap px-3 py-2 font-medium">
                    <Link
                      to="/assets/$kind/$id"
                      params={{ kind: row.kind, id: row.item.id }}
                      className="text-indigo-600 hover:underline dark:text-indigo-400"
                    >
                      {name(row)}
                    </Link>
                    {row.is_deleted && (
                      <span className="ml-2 text-xs font-normal text-slate-500">{t('asset.affectations.inTrash')}</span>
                    )}
                  </td>
                  <td className="px-3 py-2">{row.entity.name}</td>
                  <td className="px-3 py-2">{row.serial ?? '—'}</td>
                  <td className="px-3 py-2">{row.inventory_number ?? '—'}</td>
                  {canEdit && (
                    <td className="whitespace-nowrap px-3 py-2 text-right">
                      <button
                        type="button"
                        onClick={() => {
                          setNotice(null)
                          remove.reset()
                          setConfirm(row)
                        }}
                        aria-label={t('asset.affectations.removeLabel', { name: name(row) })}
                        className="text-red-700 hover:underline dark:text-red-300"
                      >
                        {t('asset.affectations.remove')}
                      </button>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}

/** Queries an affectation changes: its lists, the validity of licenses, installations. */
export function affectedBy({ queryKey }: { queryKey: readonly unknown[] }): boolean {
  const [root, kind] = queryKey
  return (
    root === 'license-items' ||
    root === 'asset-licenses' ||
    root === 'installations' ||
    ((root === 'asset' || root === 'assets') && (kind === 'software-licenses' || kind === 'softwares'))
  )
}

export function RemoveConfirm({
  name,
  pending,
  onConfirm,
  onCancel,
}: {
  name: string
  pending: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  const { t } = useTranslation()
  return (
    <div
      role="alertdialog"
      aria-label={t('asset.affectations.removeLabel', { name })}
      className="flex flex-wrap items-center gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100"
    >
      <p>{t('asset.affectations.removeConfirm', { name })}</p>
      <button
        type="button"
        disabled={pending}
        onClick={onConfirm}
        className="rounded-md bg-red-600 px-3 py-1.5 font-medium text-white hover:bg-red-500 disabled:opacity-60"
      >
        {t('asset.affectations.remove')}
      </button>
      <button
        type="button"
        onClick={onCancel}
        className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
      >
        {t('asset.form.cancel')}
      </button>
    </div>
  )
}
