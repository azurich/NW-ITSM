import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useParams, useRouterState } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import { ASSET_TYPES, assetQuery, setTrashed, type AssetType } from '../api/assets'
import { WriteError, useColumnLabel } from '../components/AssetForm'
import { LegacyLink } from '../components/LegacyLink'
import { ErrorState, Loading } from '../components/States'
import { formatDateTime } from '../lib/format'

export function AssetPage() {
  const { t, i18n } = useTranslation()
  const { kind, id } = useParams({ from: '/authenticated/assets/$kind/$id' })
  const queryClient = useQueryClient()
  const query = useQuery({ ...assetQuery(kind, id), enabled: Number.isInteger(id) })
  // Reported by the write that led here.
  const duplicates = useRouterState({ select: (state) => state.location.state.duplicates }) ?? []
  const columnLabel = useColumnLabel()
  const trash = useMutation({
    mutationFn: (trashed: boolean) => setTrashed(kind, id, trashed),
    onSuccess: async (asset) => {
      queryClient.setQueryData(assetQuery(kind, id).queryKey, asset)
      await queryClient.invalidateQueries({ queryKey: ['assets', kind] })
    },
  })

  const back = <BackLink kind={kind} />
  if (!Number.isInteger(id) || (query.error instanceof ApiError && query.error.status === 404)) {
    return (
      <div className="space-y-4">
        {back}
        <ErrorState message={t('asset.notFound')} />
      </div>
    )
  }
  if (query.isPending) {
    return <Loading />
  }
  if (query.isError) {
    return <ErrorState onRetry={() => void query.refetch()} />
  }

  const asset = query.data
  const info = ASSET_TYPES[kind]
  const legacyPath = `front/${info.legacy}.form.php?id=${asset.id}`
  const date = (value: string | null | undefined) => formatDateTime(value, i18n.language)
  const readOnly = !asset.permissions.update && !asset.permissions.trash
  const hasFeatures = info.size || info.memory || info.pageCounters || info.powerSupply || info.global || info.features

  return (
    <article className="space-y-6">
      <div className="space-y-2">
        {back}
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-2xl font-semibold">{asset.name || t('assets.unnamed')}</h1>
          {asset.is_deleted && <Badge tone="red">{t('asset.inTrash')}</Badge>}
          {asset.is_dynamic && <Badge tone="blue">{t('asset.dynamic')}</Badge>}
          {asset.is_recursive && <Badge tone="slate">{t('asset.recursive')}</Badge>}
          {info.global && asset.is_global && <Badge tone="slate">{t('asset.global')}</Badge>}
          <div className="ml-auto flex flex-wrap items-center gap-2 text-sm">
            {asset.permissions.update && !asset.is_deleted && (
              <Link
                to="/assets/$kind/$id/edit"
                params={{ kind, id: asset.id }}
                className="rounded-md bg-indigo-600 px-3 py-1.5 font-medium text-white hover:bg-indigo-500"
              >
                {t('asset.edit')}
              </Link>
            )}
            {asset.permissions.trash && (
              <button
                type="button"
                disabled={trash.isPending}
                onClick={() => trash.mutate(!asset.is_deleted)}
                className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 disabled:opacity-60 dark:border-slate-700 dark:hover:bg-slate-800"
              >
                {asset.is_deleted ? t('asset.restore') : t('asset.moveToTrash')}
              </button>
            )}
            <LegacyLink path={legacyPath} className="text-slate-500 hover:underline">
              {t('asset.openLegacy')} ↗
            </LegacyLink>
          </div>
        </div>
        {readOnly && <p className="text-sm text-slate-500">{t('asset.readOnly')}</p>}
        {trash.isSuccess && (
          <p role="status" className="text-sm text-green-700 dark:text-green-400">
            {asset.is_deleted ? t('asset.trashed') : t('asset.restored')}
          </p>
        )}
        {trash.isError && <WriteError error={trash.error} legacyPath={legacyPath} />}
        {duplicates.map((duplicate) => (
          <div
            key={duplicate.fields.join()}
            role="status"
            className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-100"
          >
            {t('asset.duplicates', { fields: duplicate.fields.map(columnLabel).join(', ') })}{' '}
            {duplicate.items.map((item, index) => (
              <span key={item.id}>
                {index > 0 && ', '}
                <Link to="/assets/$kind/$id" params={{ kind, id: item.id }} className="font-medium underline">
                  {item.name || `#${item.id}`}
                </Link>
              </span>
            ))}
          </div>
        ))}
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <Section title={t('asset.sections.general')}>
          <Field label={t('asset.fields.entity')}>{asset.entity.name}</Field>
          <Field label={t('asset.fields.type')}>{asset.type?.name}</Field>
          <Field label={t('asset.fields.manufacturer')}>{asset.manufacturer?.name}</Field>
          <Field label={t('asset.fields.model')}>{asset.model?.name}</Field>
          <Field label={t('asset.fields.serial')}>{asset.serial}</Field>
          <Field label={t('asset.fields.inventory_number')}>{asset.inventory_number}</Field>
          {info.uuid && <Field label={t('asset.fields.uuid')}>{asset.uuid}</Field>}
          {info.brand && <Field label={t('asset.fields.brand')}>{asset.brand}</Field>}
          {info.numberLine && <Field label={t('asset.fields.number_line')}>{asset.number_line}</Field>}
          {info.network && <Field label={t('asset.fields.network')}>{asset.network?.name}</Field>}
          {info.updateSource && (
            <Field label={t('asset.fields.update_source')}>{asset.update_source?.name}</Field>
          )}
          <Field label={t('asset.fields.created_at')}>{date(asset.created_at)}</Field>
          <Field label={t('asset.fields.updated_at')}>{date(asset.updated_at)}</Field>
        </Section>
        <Section title={t('asset.sections.management')}>
          <Field label={t('asset.fields.state')}>{asset.state?.name}</Field>
          <Field label={t('asset.fields.location')}>{asset.location?.name}</Field>
          <Field label={t('asset.fields.user')}>{asset.user?.name}</Field>
          <Field label={t('asset.fields.group')}>{asset.group?.name}</Field>
          <Field label={t('asset.fields.contact')}>{asset.contact}</Field>
          <Field label={t('asset.fields.contact_number')}>{asset.contact_number}</Field>
          <Field label={t('asset.fields.tech_user')}>{asset.tech_user?.name}</Field>
          <Field label={t('asset.fields.tech_group')}>{asset.tech_group?.name}</Field>
        </Section>
      </div>

      {hasFeatures && (
        <Section title={t('asset.sections.features')}>
          {info.size && <Field label={t('asset.fields.size')}>{asset.size && `${asset.size} "`}</Field>}
          {info.memory && (
            <Field label={t(info.memory === 'mib' ? 'asset.fields.memory_mib' : 'asset.fields.memory')}>
              {asset.memory}
            </Field>
          )}
          {info.pageCounters && (
            <>
              <Field label={t('asset.fields.initial_page_counter')}>{asset.initial_page_counter}</Field>
              <Field label={t('asset.fields.page_counter')}>{asset.page_counter}</Field>
            </>
          )}
          {info.powerSupply && <Field label={t('asset.fields.power_supply')}>{asset.power_supply?.name}</Field>}
          {info.global && (
            <Field label={t('asset.fields.is_global')}>
              {asset.is_global ? t('asset.management.global') : t('asset.management.unit')}
            </Field>
          )}
          {info.features?.map((feature) => (
            <Field key={feature} label={t(`asset.features.${feature}`)}>
              {asset[feature] ? t('asset.yes') : t('asset.no')}
            </Field>
          ))}
        </Section>
      )}

      {asset.comment && (
        <Section title={t('asset.fields.comment')}>
          <p className="col-span-2 whitespace-pre-wrap text-sm">{asset.comment}</p>
        </Section>
      )}
    </article>
  )
}

export function BackLink({ kind }: { kind: AssetType }) {
  const { t } = useTranslation()
  return (
    <Link
      to="/assets/$kind"
      params={{ kind }}
      className="text-sm text-indigo-600 hover:underline dark:text-indigo-400"
    >
      ← {t('asset.back', { type: t(`assets.types.${kind}`) })}
    </Link>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
      <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-slate-500">{title}</h2>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 text-sm">{children}</dl>
    </section>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-slate-500">{label}</dt>
      <dd className="break-words">{children == null || children === '' || children === false ? '—' : children}</dd>
    </>
  )
}

const TONES = {
  red: 'bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-200',
  blue: 'bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-200',
  slate: 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-200',
}

function Badge({ tone, children }: { tone: keyof typeof TONES; children: ReactNode }) {
  return <span className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${TONES[tone]}`}>{children}</span>
}
