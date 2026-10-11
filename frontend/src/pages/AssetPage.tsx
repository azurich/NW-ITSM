import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate, useParams, useRouterState } from '@tanstack/react-router'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import { ASSET_TYPES, assetQuery, purgeAsset, setTrashed, type AssetType } from '../api/assets'
import { WriteError, useColumnLabel } from '../components/AssetForm'
import { LegacyLink } from '../components/LegacyLink'
import { SoftwareVersions } from '../components/SoftwareVersions'
import { ErrorState, Loading } from '../components/States'
import { hasField, type RefField, type TextField } from '../lib/assetForm'
import { formatDateTime } from '../lib/format'
import { ORIENTATIONS, parsePosition } from '../lib/rack'

export function AssetPage() {
  const { t, i18n } = useTranslation()
  const { kind, id } = useParams({ from: '/authenticated/assets/$kind/$id' })
  const queryClient = useQueryClient()
  const query = useQuery({ ...assetQuery(kind, id), enabled: Number.isInteger(id) })
  // Reported by the write that led here.
  const duplicates = useRouterState({ select: (state) => state.location.state.duplicates }) ?? []
  const columnLabel = useColumnLabel()
  const navigate = useNavigate()
  const trash = useMutation({
    mutationFn: (trashed: boolean) => setTrashed(kind, id, trashed),
    onSuccess: async (asset) => {
      queryClient.setQueryData(assetQuery(kind, id).queryKey, asset)
      await queryClient.invalidateQueries({ queryKey: ['assets', kind] })
    },
  })
  const [confirmPurge, setConfirmPurge] = useState(false)
  const purge = useMutation({
    mutationFn: () => purgeAsset(kind, id),
    onSuccess: async () => {
      queryClient.removeQueries({ queryKey: assetQuery(kind, id).queryKey })
      await queryClient.invalidateQueries({ queryKey: ['assets', kind] })
      await navigate({ to: '/assets/$kind', params: { kind }, search: { trash: true } })
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
  const hasFeatures =
    info.size || info.memory || info.pageCounters || info.powerSupply || info.powerSupplies || info.global || info.features
  // The fields of the type only.
  const text = (field: TextField) =>
    hasField(kind, field) && <Field label={t(`asset.fields.${field}`)}>{asset[field]}</Field>
  const ref = (field: RefField, label = t(`asset.fields.${field}`)) =>
    hasField(kind, field) && <Field label={label}>{asset[field]?.name}</Field>
  const yesNo = (value: boolean | null | undefined) => (value ? t('asset.yes') : t('asset.no'))
  const place = parsePosition(asset.position)
  const orientation = ORIENTATIONS[asset.room_orientation as keyof typeof ORIENTATIONS] as
    | (typeof ORIENTATIONS)[keyof typeof ORIENTATIONS]
    | undefined

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
            {asset.is_deleted && asset.permissions.purge && !confirmPurge && (
              <button
                type="button"
                onClick={() => setConfirmPurge(true)}
                className="rounded-md border border-red-300 px-3 py-1.5 text-red-700 hover:bg-red-50 dark:border-red-800 dark:text-red-300 dark:hover:bg-red-950"
              >
                {t('asset.purge')}
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
        {confirmPurge && (
          <div
            role="alertdialog"
            aria-label={t('asset.purge')}
            className="flex flex-wrap items-center gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100"
          >
            <p>{t('asset.purgeConfirm')}</p>
            <button
              type="button"
              disabled={purge.isPending}
              onClick={() => purge.mutate()}
              className="rounded-md bg-red-600 px-3 py-1.5 font-medium text-white hover:bg-red-500 disabled:opacity-60"
            >
              {t('asset.purgeConfirmButton')}
            </button>
            <button
              type="button"
              onClick={() => {
                setConfirmPurge(false)
                purge.reset()
              }}
              className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
            >
              {t('asset.form.cancel')}
            </button>
          </div>
        )}
        {purge.isError && <WriteError error={purge.error} legacyPath={legacyPath} />}
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
          {ref('type')}
          {ref('manufacturer', info.software ? t('asset.fields.publisher') : undefined)}
          {ref('category')}
          {ref('model')}
          {text('serial')}
          {text('inventory_number')}
          {text('uuid')}
          {text('brand')}
          {text('number_line')}
          {text('version')}
          {ref('network')}
          {ref('update_source')}
          <Field label={t('asset.fields.created_at')}>{date(asset.created_at)}</Field>
          <Field label={t('asset.fields.updated_at')}>{date(asset.updated_at)}</Field>
        </Section>
        <Section title={t('asset.sections.management')}>
          {ref('state')}
          {ref('location')}
          {ref('user')}
          {ref('group')}
          {text('contact')}
          {text('contact_number')}
          {ref('tech_user')}
          {ref('tech_group')}
          {info.software && (
            <>
              <Field label={t('asset.fields.is_helpdesk_visible')}>{yesNo(asset.is_helpdesk_visible)}</Field>
              <Field label={t('asset.fields.valid_licenses')}>{yesNo(asset.valid_licenses)}</Field>
            </>
          )}
        </Section>
      </div>

      {info.software && (
        <Section title={t('asset.sections.upgrade')}>
          <Field label={t('asset.fields.is_update')}>{yesNo(asset.is_update)}</Field>
          {asset.is_update && <Field label={t('asset.fields.upgrade_from')}>{asset.upgrade_from?.name}</Field>}
        </Section>
      )}

      {info.rack && (
        <Section title={t('asset.sections.rack')}>
          {ref('room')}
          <Field label={t('asset.fields.position')}>
            {place ? t('asset.rack.position', place) : asset.position !== '0' && asset.position}
          </Field>
          <Field label={t('asset.fields.room_orientation')}>
            {orientation && t(`asset.orientations.${orientation}`)}
          </Field>
          <Field label={t('asset.fields.number_units')}>
            {asset.number_units != null && t('asset.rack.units', { count: asset.number_units })}
          </Field>
          <Field label={t('asset.fields.width')}>{asset.width}</Field>
          <Field label={t('asset.fields.height')}>{asset.height}</Field>
          <Field label={t('asset.fields.depth')}>{asset.depth}</Field>
          <Field label={t('asset.fields.max_power')}>{asset.max_power}</Field>
          <Field label={t('asset.fields.measured_power')}>{asset.measured_power}</Field>
          <Field label={t('asset.fields.max_weight')}>{asset.max_weight}</Field>
          <Field label={t('asset.fields.background_color')}>
            {asset.background_color && (
              <span className="inline-flex items-center gap-2">
                <span
                  aria-hidden="true"
                  className="inline-block size-4 rounded border border-slate-300 dark:border-slate-700"
                  style={{ backgroundColor: asset.background_color }}
                />
                {asset.background_color}
              </span>
            )}
          </Field>
        </Section>
      )}

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
          {info.powerSupplies && <Field label={t('asset.fields.power_supplies')}>{asset.power_supplies}</Field>}
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

      {info.software && (
        <SoftwareVersions softwareId={asset.id} entityId={asset.entity.id} editable={!asset.is_deleted} />
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
