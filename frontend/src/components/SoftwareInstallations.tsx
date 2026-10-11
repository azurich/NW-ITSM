import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import {
  ASSET_TYPES,
  installSoftware,
  installationsQuery,
  uninstallSoftware,
  versionsQuery,
  type AssetType,
  type Choice,
  type Installation,
} from '../api/assets'
import { formatDate } from '../lib/format'
import { WriteError } from './AssetForm'
import { ChoicePicker } from './ChoicePicker'
import { inputClass } from './PasswordFields'
import { ErrorState, Loading } from './States'

/**
 * Software installed on an asset, like its software tab in the classic
 * interface: installed from a version of a software, uninstalled for good,
 * both noted in the history of the asset and of the version.
 */
export function SoftwareInstallations({
  kind,
  assetId,
  entityId,
  editable,
}: {
  kind: AssetType
  assetId: number
  entityId: number
  /** The asset can be changed (not in the trash). */
  editable: boolean
}) {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const query = useQuery(installationsQuery(kind, assetId))
  const [software, setSoftware] = useState<Choice | null>(null)
  const [versionId, setVersionId] = useState('')
  const versions = useQuery({ ...versionsQuery(software?.id ?? 0), enabled: !!software })
  const [confirm, setConfirm] = useState<Installation | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const legacyPath = `front/${ASSET_TYPES[kind].legacy}.form.php?id=${assetId}`

  const refresh = async () => {
    await queryClient.invalidateQueries({ queryKey: installationsQuery(kind, assetId).queryKey })
    // The number of installations of the version changes.
    if (software) {
      await queryClient.invalidateQueries({ queryKey: versionsQuery(software.id).queryKey })
    }
  }
  const install = useMutation({
    mutationFn: () => installSoftware(kind, assetId, Number(versionId)),
    onSuccess: async () => {
      setSoftware(null)
      setVersionId('')
      setNotice(t('asset.installations.installed'))
      await refresh()
    },
  })
  const uninstall = useMutation({
    mutationFn: (installation: Installation) => uninstallSoftware(kind, assetId, installation.id),
    onSuccess: async () => {
      setConfirm(null)
      setNotice(t('asset.installations.uninstalled'))
      await refresh()
    },
  })

  const onSubmit = (event: FormEvent) => {
    event.preventDefault()
    setNotice(null)
    uninstall.reset()
    install.mutate()
  }
  const name = (installation: Installation) =>
    [installation.software?.name, installation.version.name].filter(Boolean).join(' ')
  const canEdit = editable && !!query.data?.can_edit
  const yesNo = (value: boolean | null | undefined) =>
    value == null ? '—' : value ? t('asset.yes') : t('asset.no')

  return (
    <section
      aria-labelledby="installations-title"
      className="space-y-3 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900"
    >
      <h2 id="installations-title" className="text-sm font-semibold uppercase tracking-wide text-slate-500">
        {t('asset.installations.title')}
      </h2>
      {notice && (
        <p role="status" className="text-sm text-green-700 dark:text-green-400">
          {notice}
        </p>
      )}

      {canEdit && (
        <form
          onSubmit={onSubmit}
          aria-label={t('asset.installations.install')}
          className="grid items-end gap-4 sm:grid-cols-[1fr_1fr_auto]"
        >
          <ChoicePicker
            kind={kind}
            field="software"
            entityId={entityId}
            label={t('asset.installations.software')}
            value={software}
            onChange={(value) => {
              setSoftware(value)
              setVersionId('')
            }}
          />
          <label className="block text-sm font-medium">
            {t('asset.installations.version')}
            <select
              value={versionId}
              required
              disabled={!software || versions.isPending}
              onChange={(event) => setVersionId(event.target.value)}
              className={inputClass}
            >
              <option value="">
                {!software
                  ? t('asset.installations.chooseSoftware')
                  : versions.data?.items.length === 0
                    ? t('asset.installations.noVersion')
                    : t('asset.form.none')}
              </option>
              {versions.data?.items.map((version) => (
                <option key={version.id} value={version.id}>
                  {version.name || t('asset.versions.unnamed')}
                  {version.state?.name ? ` (${version.state.name})` : ''}
                </option>
              ))}
            </select>
          </label>
          <button
            type="submit"
            disabled={!versionId || install.isPending}
            className="rounded-md bg-indigo-600 px-3 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-60"
          >
            {t('asset.installations.install')}
          </button>
        </form>
      )}
      {install.isError && <WriteError error={install.error} legacyPath={legacyPath} />}

      {confirm && (
        <div
          role="alertdialog"
          aria-label={t('asset.installations.uninstallLabel', { name: name(confirm) })}
          className="flex flex-wrap items-center gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100"
        >
          <p>{t('asset.installations.uninstallConfirm', { name: name(confirm) })}</p>
          <button
            type="button"
            disabled={uninstall.isPending}
            onClick={() => uninstall.mutate(confirm)}
            className="rounded-md bg-red-600 px-3 py-1.5 font-medium text-white hover:bg-red-500 disabled:opacity-60"
          >
            {t('asset.installations.uninstall')}
          </button>
          <button
            type="button"
            onClick={() => {
              setConfirm(null)
              uninstall.reset()
            }}
            className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          >
            {t('asset.form.cancel')}
          </button>
        </div>
      )}
      {uninstall.isError && <WriteError error={uninstall.error} legacyPath={legacyPath} />}

      {query.isPending ? (
        <Loading />
      ) : query.isError ? (
        <ErrorState onRetry={() => void query.refetch()} />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="border-b border-slate-200 text-xs uppercase text-slate-500 dark:border-slate-800">
              <tr>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.installations.software')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.installations.version')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.installations.state')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.installations.category')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.installations.validLicenses')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.installations.installedOn')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.installations.dynamic')}
                </th>
                {canEdit && (
                  <th scope="col" className="px-3 py-2">
                    <span className="sr-only">{t('asset.installations.actions')}</span>
                  </th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-slate-800">
              {query.data.items.length === 0 && (
                <tr>
                  <td colSpan={canEdit ? 8 : 7} className="px-3 py-6 text-center text-slate-500">
                    {t('asset.installations.empty')}
                  </td>
                </tr>
              )}
              {query.data.items.map((installation) => (
                <tr key={installation.id}>
                  <td className="whitespace-nowrap px-3 py-2 font-medium">
                    {installation.software ? (
                      <Link
                        to="/assets/$kind/$id"
                        params={{ kind: 'softwares', id: installation.software.id }}
                        className="text-indigo-600 hover:underline dark:text-indigo-400"
                      >
                        {installation.software.name || `#${installation.software.id}`}
                      </Link>
                    ) : (
                      '—'
                    )}
                  </td>
                  <td className="px-3 py-2">{installation.version.name || t('asset.versions.unnamed')}</td>
                  <td className="px-3 py-2">{installation.version_state ?? '—'}</td>
                  <td className="px-3 py-2">{installation.category?.name ?? '—'}</td>
                  <td className="px-3 py-2">{yesNo(installation.valid_licenses)}</td>
                  <td className="px-3 py-2">{formatDate(installation.installed_on, i18n.language)}</td>
                  <td className="px-3 py-2">{yesNo(installation.is_dynamic)}</td>
                  {canEdit && (
                    <td className="whitespace-nowrap px-3 py-2 text-right">
                      <button
                        type="button"
                        onClick={() => {
                          setNotice(null)
                          uninstall.reset()
                          setConfirm(installation)
                        }}
                        aria-label={t('asset.installations.uninstallLabel', { name: name(installation) })}
                        className="text-red-700 hover:underline dark:text-red-300"
                      >
                        {t('asset.installations.uninstall')}
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
