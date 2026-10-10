import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import {
  ASSET_TYPES,
  assetQuery,
  createAsset,
  updateAsset,
  type AssetDetail,
  type DuplicateNotice,
} from '../api/assets'
import { CREATE, hasRight, selectableEntitiesQuery, sessionQuery } from '../api/session'
import { AssetFormView } from '../components/AssetForm'
import { inputClass } from '../components/PasswordFields'
import { ErrorState, Loading } from '../components/States'
import { changedFields, emptyForm, formFromDetail, type AssetForm } from '../lib/assetForm'
import { activeEntities } from '../lib/entities'
import { BackLink } from './AssetPage'

/** Stores a written asset and refreshes the lists. */
function useStoreAsset() {
  const queryClient = useQueryClient()
  return async (asset: AssetDetail) => {
    queryClient.setQueryData(assetQuery(asset.kind, asset.id).queryKey, asset)
    await queryClient.invalidateQueries({ queryKey: ['assets', asset.kind] })
  }
}

export function AssetCreatePage() {
  const { t } = useTranslation()
  const { kind } = useParams({ from: '/authenticated/assets/$kind/new' })
  const { data: session } = useSuspenseQuery(sessionQuery)
  const selectable = useQuery(selectableEntitiesQuery)
  const navigate = useNavigate()
  const store = useStoreAsset()
  const [entityId, setEntityId] = useState<number | null>(null)
  const mutation = useMutation({
    mutationFn: ({ entity, form }: { entity: number; form: AssetForm }) =>
      createAsset(kind, entity, changedFields(kind, form)),
    onSuccess: async (asset) => {
      await store(asset)
      await navigate({
        to: '/assets/$kind/$id',
        params: { kind, id: asset.id },
        state: { duplicates: asset.duplicates },
      })
    },
  })

  const title = t('asset.form.createTitle', { type: t(`assets.types.${kind}`) })
  const header = (
    <div className="space-y-2">
      <BackLink kind={kind} />
      <h1 className="text-2xl font-semibold">{title}</h1>
    </div>
  )
  if (!hasRight(session, ASSET_TYPES[kind].right, CREATE)) {
    return (
      <div className="space-y-4">
        {header}
        <ErrorState message={t('asset.errors.forbidden')} />
      </div>
    )
  }
  if (selectable.isPending) {
    return <Loading />
  }
  if (selectable.isError) {
    return <ErrorState onRetry={() => void selectable.refetch()} />
  }

  const entities = activeEntities(session.entities, selectable.data)
  const [first] = entities
  if (!first) {
    return (
      <div className="space-y-4">
        {header}
        <ErrorState message={t('asset.form.noEntity')} />
      </div>
    )
  }
  const preferred = entities.find((entity) => entity.id === session.entities.selected?.id) ?? first
  const entity = entities.find((candidate) => candidate.id === entityId) ?? preferred
  const entityField = (
    <label className="block text-sm font-medium">
      {t('asset.fields.entity')}
      {entities.length === 1 ? (
        <span className="mt-1 block font-normal">{entity.completename ?? entity.name}</span>
      ) : (
        <select
          value={entity.id}
          onChange={(event) => setEntityId(Number(event.target.value))}
          className={inputClass}
        >
          {entities.map((candidate) => (
            <option key={candidate.id} value={candidate.id}>
              {candidate.completename ?? candidate.name ?? `#${candidate.id}`}
            </option>
          ))}
        </select>
      )}
    </label>
  )

  return (
    <div className="space-y-6">
      {header}
      <AssetFormView
        // The lists depend on the entity: start again when it changes.
        key={entity.id}
        kind={kind}
        entityId={entity.id}
        entityField={entityField}
        initial={emptyForm()}
        submitLabel={t('asset.form.create')}
        pending={mutation.isPending}
        error={mutation.error}
        legacyPath={`front/${ASSET_TYPES[kind].legacy}.form.php`}
        onSubmit={(form) => mutation.mutate({ entity: entity.id, form })}
        onCancel={() => void navigate({ to: '/assets/$kind', params: { kind } })}
      />
    </div>
  )
}

export function AssetEditPage() {
  const { t } = useTranslation()
  const { kind, id } = useParams({ from: '/authenticated/assets/$kind/$id/edit' })
  const query = useQuery({ ...assetQuery(kind, id), enabled: Number.isInteger(id) })
  const navigate = useNavigate()
  const store = useStoreAsset()
  const toDetail = (duplicates?: DuplicateNotice[]) =>
    navigate({ to: '/assets/$kind/$id', params: { kind, id }, state: { duplicates } })
  const mutation = useMutation({
    mutationFn: async ({ form, initial }: { form: AssetForm; initial: AssetForm }) => {
      const fields = changedFields(kind, form, initial)
      return Object.keys(fields).length > 0 ? updateAsset(kind, id, fields) : null
    },
    onSuccess: async (asset) => {
      if (asset) {
        await store(asset)
      }
      await toDetail(asset?.duplicates)
    },
  })

  if (!Number.isInteger(id) || (query.error instanceof ApiError && query.error.status === 404)) {
    return (
      <div className="space-y-4">
        <BackLink kind={kind} />
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
  const header = (
    <div className="space-y-2">
      <BackLink kind={kind} />
      <h1 className="text-2xl font-semibold">
        {t('asset.form.editTitle', { name: asset.name || t('assets.unnamed') })}
      </h1>
      <p className="text-sm text-slate-500">{asset.entity.name}</p>
    </div>
  )
  if (!asset.permissions.update || asset.is_deleted) {
    return (
      <div className="space-y-4">
        {header}
        <ErrorState message={asset.is_deleted ? t('asset.form.inTrash') : t('asset.errors.forbidden')} />
      </div>
    )
  }

  const initial = formFromDetail(asset)
  return (
    <div className="space-y-6">
      {header}
      <AssetFormView
        key={asset.updated_at ?? ''}
        kind={kind}
        entityId={asset.entity.id}
        initial={initial}
        submitLabel={t('asset.form.save')}
        pending={mutation.isPending}
        error={mutation.error}
        legacyPath={`front/${ASSET_TYPES[kind].legacy}.form.php?id=${asset.id}`}
        onSubmit={(form) => mutation.mutate({ form, initial })}
        onCancel={() => void toDetail()}
      />
    </div>
  )
}
