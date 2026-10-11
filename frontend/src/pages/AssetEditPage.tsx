import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import { Link, useNavigate, useParams, useSearch } from '@tanstack/react-router'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import {
  ASSET_TYPES,
  assetQuery,
  createAsset,
  createFromTemplate,
  createTemplate,
  defaultsQuery,
  templateQuery,
  templatesQuery,
  updateAsset,
  updateTemplate,
  type AssetDetail,
  type AssetType,
  type Choice,
  type DuplicateNotice,
  type TemplateSummary,
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

/**
 * Creation of an item: first the choice of a template when the type has
 * some (like the classic interface), then the form, blank or filled with
 * the values of the template.
 */
export function AssetCreatePage() {
  const { t } = useTranslation()
  const { kind } = useParams({ from: '/authenticated/assets/$kind/new' })
  const { template, software, software_name } = useSearch({ from: '/authenticated/assets/$kind/new' })
  const { data: session } = useSuspenseQuery(sessionQuery)
  const withTemplates = !ASSET_TYPES[kind].noTemplates
  const templates = useQuery({ ...templatesQuery(kind), enabled: withTemplates && template === undefined })

  const header = (
    <div className="space-y-2">
      <BackLink kind={kind} />
      <h1 className="text-2xl font-semibold">{t('asset.form.createTitle', { type: t(`assets.types.${kind}`) })}</h1>
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
  if (withTemplates && template === undefined) {
    if (templates.isPending) {
      return <Loading />
    }
    if (templates.isError) {
      return <ErrorState onRetry={() => void templates.refetch()} />
    }
    if (templates.data.items.length > 0) {
      return (
        <div className="space-y-6">
          {header}
          <TemplateChooser kind={kind} templates={templates.data.items} />
        </div>
      )
    }
  }
  return template ? (
    <CreateFromTemplate kind={kind} templateId={template} header={header} />
  ) : (
    <CreateBlank
      kind={kind}
      header={header}
      software={software ? { id: software, label: software_name ?? `#${software}` } : undefined}
    />
  )
}

/** `CommonDBTM::listTemplates` with `add`: a blank form or a template. */
function TemplateChooser({ kind, templates }: { kind: AssetType; templates: TemplateSummary[] }) {
  const { t } = useTranslation()
  const linkClass = 'font-medium text-indigo-600 hover:underline dark:text-indigo-400'
  return (
    <section className="space-y-3">
      <h2 className="text-lg font-medium">{t('templates.choose')}</h2>
      <ul className="divide-y divide-slate-100 rounded-lg border border-slate-200 bg-white dark:divide-slate-800 dark:border-slate-800 dark:bg-slate-900">
        <li className="px-4 py-3">
          <Link to="/assets/$kind/new" params={{ kind }} search={{ template: 0 }} className={linkClass}>
            {t('templates.blank')}
          </Link>
        </li>
        {templates.map((template) => (
          <li key={template.id} className="flex flex-wrap items-baseline gap-x-3 px-4 py-3">
            <Link to="/assets/$kind/new" params={{ kind }} search={{ template: template.id }} className={linkClass}>
              {template.template_name || t('templates.unnamed', { id: template.id })}
            </Link>
            <span className="text-sm text-slate-500">{template.entity.name}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}

/**
 * Form of a new item or template, as the classic one starts (`getEmpty`):
 * `null` while the settings it depends on load.
 */
function useNewForm(kind: AssetType): { form: AssetForm | null; state?: ReactNode } {
  const needed = !!ASSET_TYPES[kind].software
  const defaults = useQuery({ ...defaultsQuery(kind), enabled: needed })
  if (!needed) {
    return { form: emptyForm(kind) }
  }
  if (defaults.isPending) {
    return { form: null, state: <Loading /> }
  }
  if (defaults.isError) {
    return { form: null, state: <ErrorState onRetry={() => void defaults.refetch()} /> }
  }
  return { form: emptyForm(kind, defaults.data) }
}

/** Entity of a new item or template: one of the selected entities. */
function useEntityChoice() {
  const { t } = useTranslation()
  const { data: session } = useSuspenseQuery(sessionQuery)
  const selectable = useQuery(selectableEntitiesQuery)
  const [entityId, setEntityId] = useState<number | null>(null)
  if (selectable.isPending) {
    return { state: <Loading /> }
  }
  if (selectable.isError) {
    return { state: <ErrorState onRetry={() => void selectable.refetch()} /> }
  }
  const entities = activeEntities(session.entities, selectable.data)
  const [first] = entities
  if (!first) {
    return { state: <ErrorState message={t('asset.form.noEntity')} /> }
  }
  const preferred = entities.find((entity) => entity.id === session.entities.selected?.id) ?? first
  const entity = entities.find((candidate) => candidate.id === entityId) ?? preferred
  const field = (
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
  return { entity, field }
}

function CreateBlank({ kind, header, software }: { kind: AssetType; header: ReactNode; software?: Choice }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const store = useStoreAsset()
  const choice = useEntityChoice()
  const blank = useNewForm(kind)
  const mutation = useMutation({
    // What differs from the form of a new item: the API gives the rest.
    mutationFn: ({ entity, form, initial }: { entity: number; form: AssetForm; initial: AssetForm }) =>
      createAsset(kind, entity, changedFields(kind, form, initial)),
    onSuccess: async (asset) => {
      await store(asset)
      await navigate({
        to: '/assets/$kind/$id',
        params: { kind, id: asset.id },
        state: { duplicates: asset.duplicates },
      })
    },
  })
  if (!choice.entity || !blank.form) {
    return (
      <div className="space-y-4">
        {header}
        {choice.state ?? blank.state}
      </div>
    )
  }
  const { entity } = choice
  const initial = blank.form
  // A license of the software whose page it was added from.
  const start =
    software && ASSET_TYPES[kind].license ? { ...initial, refs: { ...initial.refs, software } } : initial
  return (
    <div className="space-y-6">
      {header}
      <AssetFormView
        // The lists depend on the entity: start again when it changes.
        key={entity.id}
        kind={kind}
        entityId={entity.id}
        entityField={choice.field}
        initial={start}
        submitLabel={t('asset.form.create')}
        pending={mutation.isPending}
        error={mutation.error}
        legacyPath={`front/${ASSET_TYPES[kind].legacy}.form.php`}
        onSubmit={(form) => mutation.mutate({ entity: entity.id, form, initial })}
        onCancel={() => void navigate({ to: '/assets/$kind', params: { kind } })}
      />
    </div>
  )
}

/**
 * An item created from a template (`withtemplate=2`): in the entity of the
 * template, its values filling the form.
 */
function CreateFromTemplate({ kind, templateId, header }: { kind: AssetType; templateId: number; header: ReactNode }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const store = useStoreAsset()
  const query = useQuery(templateQuery(kind, templateId))
  const mutation = useMutation({
    mutationFn: ({ form, initial }: { form: AssetForm; initial: AssetForm }) =>
      createFromTemplate(kind, templateId, changedFields(kind, form, initial)),
    onSuccess: async (asset) => {
      await store(asset)
      await navigate({
        to: '/assets/$kind/$id',
        params: { kind, id: asset.id },
        state: { duplicates: asset.duplicates },
      })
    },
  })
  if (query.error instanceof ApiError && query.error.status === 404) {
    return (
      <div className="space-y-4">
        {header}
        <ErrorState message={t('templates.notFound')} />
      </div>
    )
  }
  if (query.isPending) {
    return <Loading />
  }
  if (query.isError) {
    return <ErrorState onRetry={() => void query.refetch()} />
  }
  const template = query.data
  // The fields of an item: the name of the template is not one.
  const initial: AssetForm = { ...formFromDetail(template), template_name: undefined }
  return (
    <div className="space-y-6">
      {header}
      <p className="text-sm text-slate-500">
        {t('templates.createdFrom', {
          name: template.template_name || t('templates.unnamed', { id: template.id }),
        })}
      </p>
      <AssetFormView
        key={template.id}
        kind={kind}
        entityId={template.entity.id}
        entityField={
          <div className="text-sm font-medium">
            {t('asset.fields.entity')}
            <span className="mt-1 block font-normal">{template.entity.name}</span>
          </div>
        }
        initial={initial}
        submitLabel={t('asset.form.create')}
        pending={mutation.isPending}
        error={mutation.error}
        legacyPath={`front/${ASSET_TYPES[kind].legacy}.form.php?id=${template.id}&withtemplate=2`}
        onSubmit={(form) => mutation.mutate({ form, initial })}
        onCancel={() => void navigate({ to: '/assets/$kind', params: { kind } })}
      />
    </div>
  )
}

/** Creation of a template, in one of the selected entities. */
export function TemplateCreatePage() {
  const { t } = useTranslation()
  const { kind } = useParams({ from: '/authenticated/assets/$kind/templates/new' })
  const { data: session } = useSuspenseQuery(sessionQuery)
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const choice = useEntityChoice()
  const blank = useNewForm(kind)
  const toList = () => navigate({ to: '/assets/$kind/templates', params: { kind } })
  const mutation = useMutation({
    mutationFn: ({ entity, form }: { entity: number; form: AssetForm }) =>
      createTemplate(kind, entity, {
        ...changedFields(kind, form),
        // The switches the settings set, as shown.
        ...(ASSET_TYPES[kind].software ? { is_helpdesk_visible: form.flags.is_helpdesk_visible ?? false } : {}),
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['templates', kind] })
      await toList()
    },
  })
  const header = (
    <div className="space-y-2">
      <TemplatesBackLink kind={kind} />
      <h1 className="text-2xl font-semibold">{t('templates.createTitle', { type: t(`assets.types.${kind}`) })}</h1>
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
  if (!choice.entity || !blank.form) {
    return (
      <div className="space-y-4">
        {header}
        {choice.state ?? blank.state}
      </div>
    )
  }
  const { entity } = choice
  return (
    <div className="space-y-6">
      {header}
      <AssetFormView
        key={entity.id}
        kind={kind}
        entityId={entity.id}
        entityField={choice.field}
        initial={{ ...blank.form, template_name: '' }}
        submitLabel={t('asset.form.create')}
        pending={mutation.isPending}
        error={mutation.error}
        legacyPath={`front/${ASSET_TYPES[kind].legacy}.form.php?withtemplate=1`}
        onSubmit={(form) => mutation.mutate({ entity: entity.id, form })}
        onCancel={() => void toList()}
      />
    </div>
  )
}

/** Changes of a template of a selected entity. */
export function TemplateEditPage() {
  const { t } = useTranslation()
  const { kind, id } = useParams({ from: '/authenticated/assets/$kind/templates/$id/edit' })
  const query = useQuery({ ...templateQuery(kind, id), enabled: Number.isInteger(id) })
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const toList = () => navigate({ to: '/assets/$kind/templates', params: { kind } })
  const mutation = useMutation({
    mutationFn: async ({ form, initial }: { form: AssetForm; initial: AssetForm }) => {
      const fields = changedFields(kind, form, initial)
      return Object.keys(fields).length > 0 ? updateTemplate(kind, id, fields) : null
    },
    onSuccess: async (template) => {
      if (template) {
        queryClient.setQueryData(templateQuery(kind, id).queryKey, template)
        await queryClient.invalidateQueries({ queryKey: ['templates', kind] })
      }
      await toList()
    },
  })

  const back = <TemplatesBackLink kind={kind} />
  if (!Number.isInteger(id) || (query.error instanceof ApiError && query.error.status === 404)) {
    return (
      <div className="space-y-4">
        {back}
        <ErrorState message={t('templates.notFound')} />
      </div>
    )
  }
  if (query.isPending) {
    return <Loading />
  }
  if (query.isError) {
    return <ErrorState onRetry={() => void query.refetch()} />
  }
  const template = query.data
  const header = (
    <div className="space-y-2">
      {back}
      <h1 className="text-2xl font-semibold">
        {t('templates.editTitle', { name: template.template_name || t('templates.unnamed', { id: template.id }) })}
      </h1>
      <p className="text-sm text-slate-500">{template.entity.name}</p>
    </div>
  )
  if (!template.permissions.update) {
    return (
      <div className="space-y-4">
        {header}
        <ErrorState message={t('asset.errors.forbidden')} />
      </div>
    )
  }
  const initial = formFromDetail(template)
  return (
    <div className="space-y-6">
      {header}
      <AssetFormView
        key={template.updated_at ?? ''}
        kind={kind}
        entityId={template.entity.id}
        initial={initial}
        submitLabel={t('asset.form.save')}
        pending={mutation.isPending}
        error={mutation.error}
        legacyPath={`front/${ASSET_TYPES[kind].legacy}.form.php?id=${template.id}&withtemplate=1`}
        onSubmit={(form) => mutation.mutate({ form, initial })}
        onCancel={() => void toList()}
      />
    </div>
  )
}

export function TemplatesBackLink({ kind }: { kind: AssetType }) {
  const { t } = useTranslation()
  return (
    <Link
      to="/assets/$kind/templates"
      params={{ kind }}
      className="text-sm text-indigo-600 hover:underline dark:text-indigo-400"
    >
      ← {t('templates.title', { type: t(`assets.types.${kind}`) })}
    </Link>
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
        itemId={asset.id}
      />
    </div>
  )
}
