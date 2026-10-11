import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import {
  ASSET_TYPES,
  addAssetLicense,
  assetLicensesQuery,
  removeAssetLicense,
  type AssetLicense,
  type AssetType,
  type Choice,
} from '../api/assets'
import { formatDate } from '../lib/format'
import { WriteError } from './AssetForm'
import { AssetSearchPicker } from './AssetSearchPicker'
import { RemoveConfirm, affectedBy } from './LicenseItems'
import { ErrorState, Loading } from './States'

/**
 * Licenses affected to an item with software, like the licenses of its
 * software tab: affected and removed as from the license.
 */
export function AssetLicenses({ kind, assetId, editable }: { kind: AssetType; assetId: number; editable: boolean }) {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const query = useQuery(assetLicensesQuery(kind, assetId))
  const [license, setLicense] = useState<Choice | null>(null)
  const [confirm, setConfirm] = useState<AssetLicense | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const legacyPath = `front/${ASSET_TYPES[kind].legacy}.form.php?id=${assetId}`

  const refresh = () => queryClient.invalidateQueries({ predicate: affectedBy })
  const affect = useMutation({
    mutationFn: () => addAssetLicense(kind, assetId, license?.id ?? 0),
    onSuccess: async () => {
      setLicense(null)
      setNotice(t('asset.affectations.affected'))
      await refresh()
    },
  })
  const remove = useMutation({
    mutationFn: (row: AssetLicense) => removeAssetLicense(kind, assetId, row.id),
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
  const name = (row: AssetLicense) => row.license.name || `#${row.license.id}`
  const canEdit = editable && !!query.data?.can_edit
  const link = 'font-medium text-indigo-600 hover:underline dark:text-indigo-400'

  return (
    <section
      aria-labelledby="asset-licenses-title"
      className="space-y-3 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900"
    >
      <h2 id="asset-licenses-title" className="text-sm font-semibold uppercase tracking-wide text-slate-500">
        {t('asset.affectations.licensesTitle')}
      </h2>
      {notice && (
        <p role="status" className="text-sm text-green-700 dark:text-green-400">
          {notice}
        </p>
      )}

      {canEdit && (
        <form
          onSubmit={onSubmit}
          aria-label={t('asset.affectations.affectLicense')}
          className="grid items-end gap-4 sm:grid-cols-[1fr_auto]"
        >
          <AssetSearchPicker
            kind="software-licenses"
            label={t('asset.affectations.license')}
            value={license}
            onChange={setLicense}
          />
          <button
            type="submit"
            disabled={!license || affect.isPending}
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
                {(['license', 'software', 'number', 'validity', 'expire'] as const).map((column) => (
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
                    {t('asset.affectations.noLicense')}
                  </td>
                </tr>
              )}
              {query.data.items.map((row) => (
                <tr key={row.id}>
                  <td className="whitespace-nowrap px-3 py-2">
                    <Link
                      to="/assets/$kind/$id"
                      params={{ kind: 'software-licenses', id: row.license.id }}
                      className={link}
                    >
                      {name(row)}
                    </Link>
                  </td>
                  <td className="whitespace-nowrap px-3 py-2">
                    {row.software ? (
                      <Link to="/assets/$kind/$id" params={{ kind: 'softwares', id: row.software.id }} className={link}>
                        {row.software.name || `#${row.software.id}`}
                      </Link>
                    ) : (
                      '—'
                    )}
                  </td>
                  <td className="px-3 py-2">{row.number}</td>
                  <td className="px-3 py-2">{row.is_valid ? t('asset.license.valid') : t('asset.license.invalid')}</td>
                  <td className="px-3 py-2">{formatDate(row.expire, i18n.language, t('asset.license.neverExpire'))}</td>
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
