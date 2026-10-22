import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import {
  createVersion,
  purgeVersion,
  updateVersion,
  versionsQuery,
  type Choice,
  type SoftwareVersion,
} from '../api/assets'
import { WriteError } from './AssetForm'
import { ChoicePicker } from './ChoicePicker'
import { inputClass } from './PasswordFields'
import { ErrorState, Loading } from './States'

interface VersionForm {
  name: string
  comment: string
  state: Choice | null
  operatingSystem: Choice | null
}

const EMPTY: VersionForm = { name: '', comment: '', state: null, operatingSystem: null }

function formOf(version: SoftwareVersion): VersionForm {
  const choice = (ref: SoftwareVersion['state']) => (ref ? { id: ref.id, label: ref.name ?? '' } : null)
  return {
    name: version.name ?? '',
    comment: version.comment ?? '',
    state: choice(version.state),
    operatingSystem: choice(version.operating_system),
  }
}

/** The version being added (`new`) or changed (its id). */
type Editing = 'new' | number | null

/**
 * Versions of a software: in its entity and with its sharing, added,
 * changed and deleted with the update right on the software, noted in its
 * history like the classic interface does.
 */
export function SoftwareVersions({
  softwareId,
  entityId,
  editable,
}: {
  softwareId: number
  entityId: number
  /** The software can be changed (not in the trash). */
  editable: boolean
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const query = useQuery(versionsQuery(softwareId))
  const [editing, setEditing] = useState<Editing>(null)
  const [form, setForm] = useState<VersionForm>(EMPTY)
  const [confirmPurge, setConfirmPurge] = useState<SoftwareVersion | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const refresh = async () => {
    // The history and the date of the software change too.
    await queryClient.invalidateQueries({ queryKey: versionsQuery(softwareId).queryKey })
    await queryClient.invalidateQueries({ queryKey: ['asset', 'softwares', softwareId] })
  }
  const save = useMutation({
    mutationFn: async (target: 'new' | number) => {
      // All the fields, like the classic form posts them.
      const fields = {
        name: form.name,
        comment: form.comment,
        state_id: form.state?.id ?? null,
        operating_system_id: form.operatingSystem?.id ?? null,
      }
      return target === 'new' ? createVersion(softwareId, fields) : updateVersion(softwareId, target, fields)
    },
    onSuccess: async (_, target) => {
      setEditing(null)
      setNotice(target === 'new' ? t('asset.versions.added') : t('asset.versions.saved'))
      await refresh()
    },
  })
  const purge = useMutation({
    mutationFn: (version: SoftwareVersion) => purgeVersion(softwareId, version.id),
    onSuccess: async () => {
      setConfirmPurge(null)
      setNotice(t('asset.versions.purged'))
      await refresh()
    },
  })

  const edit = (target: Editing, values: VersionForm) => {
    setEditing(target)
    setForm(values)
    setConfirmPurge(null)
    setNotice(null)
    save.reset()
    purge.reset()
  }
  const onSubmit = (event: FormEvent) => {
    event.preventDefault()
    if (editing !== null) {
      save.mutate(editing)
    }
  }
  const versionName = (version: SoftwareVersion) => version.name || t('asset.versions.unnamed')
  const canEdit = editable && !!query.data?.can_edit
  const legacyPath = (version?: SoftwareVersion) =>
    version ? `front/softwareversion.form.php?id=${version.id}` : `front/software.form.php?id=${softwareId}`
  const editedVersion = query.data?.items.find((version) => version.id === editing)
  const formTitle =
    editing === 'new'
      ? t('asset.versions.addTitle')
      : t('asset.versions.editTitle', { name: editedVersion ? versionName(editedVersion) : '' })

  return (
    <section
      aria-labelledby="versions-title"
      className="space-y-3 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900"
    >
      <div className="flex flex-wrap items-center gap-3">
        <h2 id="versions-title" className="text-sm font-semibold uppercase tracking-wide text-slate-500">
          {t('asset.versions.title')}
        </h2>
        {canEdit && editing === null && (
          <button
            type="button"
            onClick={() => edit('new', EMPTY)}
            className="ml-auto rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
          >
            {t('asset.versions.add')}
          </button>
        )}
      </div>
      {notice && (
        <p role="status" className="text-sm text-green-700 dark:text-green-400">
          {notice}
        </p>
      )}

      {editing !== null && (
        <form
          onSubmit={onSubmit}
          aria-label={formTitle}
          className="space-y-4 rounded-md border border-slate-200 p-4 dark:border-slate-700"
        >
          <h3 className="text-sm font-semibold">{formTitle}</h3>
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="block text-sm font-medium">
              {t('asset.versions.name')}
              <input
                value={form.name}
                maxLength={255}
                onChange={(event) => setForm({ ...form, name: event.target.value })}
                className={inputClass}
              />
            </label>
            <ChoicePicker
              kind="softwares"
              field="version_state"
              entityId={entityId}
              label={t('asset.versions.state')}
              value={form.state}
              onChange={(state) => setForm({ ...form, state })}
            />
            <ChoicePicker
              kind="softwares"
              field="version_operating_system"
              entityId={entityId}
              label={t('asset.versions.operating_system')}
              value={form.operatingSystem}
              onChange={(operatingSystem) => setForm({ ...form, operatingSystem })}
            />
            <label className="block text-sm font-medium sm:col-span-2">
              {t('asset.versions.comment')}
              <textarea
                rows={3}
                value={form.comment}
                onChange={(event) => setForm({ ...form, comment: event.target.value })}
                className={inputClass}
              />
            </label>
          </div>
          {save.isError && <WriteError error={save.error} legacyPath={legacyPath(editedVersion)} />}
          <div className="flex gap-2 text-sm">
            <button
              type="submit"
              disabled={save.isPending}
              className="rounded-md bg-indigo-600 px-3 py-1.5 font-medium text-white hover:bg-indigo-500 disabled:opacity-60"
            >
              {save.isPending ? t('asset.form.saving') : editing === 'new' ? t('asset.form.create') : t('asset.form.save')}
            </button>
            <button
              type="button"
              onClick={() => setEditing(null)}
              className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
            >
              {t('asset.form.cancel')}
            </button>
          </div>
        </form>
      )}

      {confirmPurge && (
        <div
          role="alertdialog"
          aria-label={t('asset.versions.purgeLabel', { name: versionName(confirmPurge) })}
          className="flex flex-wrap items-center gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100"
        >
          <p>
            {t('asset.versions.purgeConfirm', { name: versionName(confirmPurge) })}
            {confirmPurge.installations > 0 &&
              ` ${t('asset.versions.purgeInstalled', { count: confirmPurge.installations })}`}
          </p>
          <button
            type="button"
            disabled={purge.isPending}
            onClick={() => purge.mutate(confirmPurge)}
            className="rounded-md bg-red-600 px-3 py-1.5 font-medium text-white hover:bg-red-500 disabled:opacity-60"
          >
            {t('asset.purgeConfirmButton')}
          </button>
          <button
            type="button"
            onClick={() => {
              setConfirmPurge(null)
              purge.reset()
            }}
            className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          >
            {t('asset.form.cancel')}
          </button>
        </div>
      )}
      {purge.isError && <WriteError error={purge.error} legacyPath={legacyPath(purge.variables)} />}

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
                  {t('asset.versions.name')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.versions.state')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.versions.operating_system')}
                </th>
                <th scope="col" className="px-3 py-2 text-right font-medium">
                  {t('asset.versions.installations')}
                </th>
                <th scope="col" className="px-3 py-2 font-medium">
                  {t('asset.versions.comment')}
                </th>
                {canEdit && (
                  <th scope="col" className="px-3 py-2">
                    <span className="sr-only">{t('asset.versions.actions')}</span>
                  </th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-slate-800">
              {query.data.items.length === 0 && (
                <tr>
                  <td colSpan={canEdit ? 6 : 5} className="px-3 py-6 text-center text-slate-500">
                    {t('asset.versions.empty')}
                  </td>
                </tr>
              )}
              {query.data.items.map((version) => (
                <tr key={version.id}>
                  <td className="whitespace-nowrap px-3 py-2 font-medium">{versionName(version)}</td>
                  <td className="px-3 py-2">{version.state?.name ?? '—'}</td>
                  <td className="px-3 py-2">{version.operating_system?.name ?? '—'}</td>
                  <td className="px-3 py-2 text-right tabular-nums">{version.installations}</td>
                  <td className="max-w-xs truncate px-3 py-2" title={version.comment ?? undefined}>
                    {version.comment || '—'}
                  </td>
                  {canEdit && (
                    <td className="whitespace-nowrap px-3 py-2 text-right">
                      <button
                        type="button"
                        onClick={() => edit(version.id, formOf(version))}
                        aria-label={t('asset.versions.editTitle', { name: versionName(version) })}
                        className="mr-3 text-indigo-600 hover:underline dark:text-indigo-400"
                      >
                        {t('asset.edit')}
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setEditing(null)
                          setNotice(null)
                          purge.reset()
                          setConfirmPurge(version)
                        }}
                        aria-label={t('asset.versions.purgeLabel', { name: versionName(version) })}
                        className="text-red-700 hover:underline dark:text-red-300"
                      >
                        {t('asset.versions.purge')}
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
