import { useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import { ASSET_TYPES, type AssetType, type ChoiceField, type Feature } from '../api/assets'
import {
  flagFields,
  hasField,
  type AssetForm,
  type NumberField,
  type RefField,
  type TextField,
} from '../lib/assetForm'
import { ChoicePicker } from './ChoicePicker'
import { LegacyLink } from './LegacyLink'
import { inputClass } from './PasswordFields'
import { RackFields } from './RackFields'

const sectionClass = 'rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900'

/** Form of an asset, for a creation or an update. */
export function AssetFormView({
  kind,
  entityId,
  entityField,
  initial,
  submitLabel,
  pending,
  error,
  legacyPath,
  onSubmit,
  onCancel,
}: {
  kind: AssetType
  /** Entity whose values are offered in the lists. */
  entityId: number
  /** Entity choice of a creation. */
  entityField?: ReactNode
  initial: AssetForm
  submitLabel: string
  pending: boolean
  error: unknown
  /** Same form in the classic interface. */
  legacyPath: string
  onSubmit: (form: AssetForm) => void
  onCancel: () => void
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState(initial)

  const info = ASSET_TYPES[kind]
  const text = (field: TextField, label = t(`asset.fields.${field}`), inputMode?: 'decimal' | 'numeric') =>
    hasField(kind, field) && (
      <label className="block text-sm font-medium">
        {label}
        <input
          type="text"
          inputMode={inputMode}
          value={form.text[field]}
          maxLength={255}
          onChange={(event) => setForm({ ...form, text: { ...form.text, [field]: event.target.value } })}
          className={inputClass}
        />
      </label>
    )
  const number = (field: NumberField, min?: number) =>
    hasField(kind, field) && (
      <label className="block text-sm font-medium">
        {t(`asset.fields.${field}`)}
        <input
          type="number"
          step={1}
          min={min}
          value={form.numbers[field]}
          onChange={(event) =>
            setForm({ ...form, numbers: { ...form.numbers, [field]: event.target.value } })
          }
          className={inputClass}
        />
      </label>
    )
  const flag = (field: Feature) => (
    <label key={field} className="flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={form.flags[field] ?? false}
        onChange={(event) => setForm({ ...form, flags: { ...form.flags, [field]: event.target.checked } })}
      />
      {t(`asset.features.${field}`)}
    </label>
  )
  const hasFeatures =
    info.size || info.memory || info.pageCounters || info.powerSupply || info.powerSupplies || flagFields(kind).length > 0
  const choice = (field: RefField) =>
    hasField(kind, field) && (
      <ChoicePicker
        kind={kind}
        field={field as ChoiceField}
        entityId={entityId}
        label={t(`asset.fields.${field}`)}
        value={form.refs[field]}
        onChange={(value) => setForm({ ...form, refs: { ...form.refs, [field]: value } })}
      />
    )
  const submit = (event: FormEvent) => {
    event.preventDefault()
    onSubmit(form)
  }

  return (
    <form onSubmit={submit} className="space-y-6">
      <div className="grid gap-6 lg:grid-cols-2">
        <fieldset className={`${sectionClass} space-y-4`}>
          <legend className="px-1 text-sm font-semibold uppercase tracking-wide text-slate-500">
            {t('asset.sections.general')}
          </legend>
          {form.template_name !== undefined && (
            <label className="block text-sm font-medium">
              {t('templates.name')}
              <input
                type="text"
                value={form.template_name}
                maxLength={255}
                onChange={(event) => setForm({ ...form, template_name: event.target.value })}
                className={inputClass}
              />
            </label>
          )}
          {entityField}
          {text('name')}
          {choice('type')}
          {choice('manufacturer')}
          {choice('model')}
          {text('serial')}
          {text('inventory_number')}
          {text('uuid')}
          {text('brand')}
          {text('number_line')}
          {text('version')}
          {choice('network')}
          {choice('update_source')}
        </fieldset>
        <fieldset className={`${sectionClass} space-y-4`}>
          <legend className="px-1 text-sm font-semibold uppercase tracking-wide text-slate-500">
            {t('asset.sections.management')}
          </legend>
          {choice('state')}
          {choice('location')}
          {choice('user')}
          {choice('group')}
          {text('contact')}
          {text('contact_number')}
          {choice('tech_user')}
          {choice('tech_group')}
          <div>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={form.is_recursive}
                onChange={(event) => setForm({ ...form, is_recursive: event.target.checked })}
              />
              {t('asset.fields.is_recursive')}
            </label>
          </div>
        </fieldset>
      </div>

      {info.rack && (
        <fieldset className={`${sectionClass} space-y-4`}>
          <legend className="px-1 text-sm font-semibold uppercase tracking-wide text-slate-500">
            {t('asset.sections.rack')}
          </legend>
          <RackFields kind={kind} entityId={entityId} form={form} initial={initial} onChange={setForm} />
        </fieldset>
      )}

      {hasFeatures && (
        <fieldset className={`${sectionClass} space-y-4`}>
          <legend className="px-1 text-sm font-semibold uppercase tracking-wide text-slate-500">
            {t('asset.sections.features')}
          </legend>
          <div className="grid gap-4 sm:grid-cols-2">
            {text('size', undefined, 'decimal')}
            {text(
              'memory',
              t(info.memory === 'mib' ? 'asset.fields.memory_mib' : 'asset.fields.memory'),
              info.memory === 'mib' ? 'numeric' : undefined,
            )}
            {number('initial_page_counter')}
            {number('page_counter')}
            {number('power_supplies', 0)}
            {choice('power_supply')}
            {info.global && (
              <label className="block text-sm font-medium">
                {t('asset.fields.is_global')}
                <select
                  value={form.flags.is_global ? '1' : '0'}
                  onChange={(event) =>
                    setForm({ ...form, flags: { ...form.flags, is_global: event.target.value === '1' } })
                  }
                  className={inputClass}
                >
                  <option value="0">{t('asset.management.unit')}</option>
                  <option value="1">{t('asset.management.global')}</option>
                </select>
              </label>
            )}
          </div>
          {info.features && (
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">{info.features.map(flag)}</div>
          )}
        </fieldset>
      )}

      <label className={`${sectionClass} block text-sm font-medium`}>
        {t('asset.fields.comment')}
        <textarea
          rows={4}
          value={form.text.comment}
          onChange={(event) => setForm({ ...form, text: { ...form.text, comment: event.target.value } })}
          className={inputClass}
        />
      </label>

      {error != null && <WriteError error={error} legacyPath={legacyPath} />}

      <div className="flex items-center gap-3">
        <button
          type="submit"
          disabled={pending}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-60"
        >
          {pending ? t('asset.form.saving') : submitLabel}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md border border-slate-300 px-4 py-2 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
        >
          {t('asset.form.cancel')}
        </button>
      </div>
    </form>
  )
}

const LEGACY_REASONS = [
  'business_rules',
  'field_unicity',
  'linked_data',
  'linked_items',
  'notifications',
  'plugins',
] as const

/** Form fields by column, to name the fields of a unicity criterion. */
const FIELD_OF_COLUMN: Record<string, string> = {
  name: 'name',
  serial: 'serial',
  otherserial: 'inventory_number',
  uuid: 'uuid',
  brand: 'brand',
  number_line: 'number_line',
  contact: 'contact',
  contact_num: 'contact_number',
  locations_id: 'location',
  states_id: 'state',
  manufacturers_id: 'manufacturer',
  networks_id: 'network',
  autoupdatesystems_id: 'update_source',
  users_id: 'user',
  groups_id: 'group',
  users_id_tech: 'tech_user',
  groups_id_tech: 'tech_group',
  size: 'size',
  memory_size: 'memory',
  ram: 'memory_mib',
  init_pages_counter: 'initial_page_counter',
  last_pages_counter: 'page_counter',
  phonepowersupplies_id: 'power_supply',
  is_global: 'is_global',
  version: 'version',
  dcrooms_id: 'room',
  position: 'position',
  room_orientation: 'room_orientation',
  number_units: 'number_units',
  width: 'width',
  height: 'height',
  depth: 'depth',
  max_power: 'max_power',
  mesured_power: 'measured_power',
  max_weight: 'max_weight',
  bgcolor: 'background_color',
  power_supplies: 'power_supplies',
}

/** Features by column. */
const FEATURE_OF_COLUMN: Record<string, Feature> = {
  have_micro: 'has_microphone',
  have_speaker: 'has_speaker',
  have_hp: 'has_speaker',
  have_subd: 'has_subd',
  have_bnc: 'has_bnc',
  have_dvi: 'has_dvi',
  have_pivot: 'has_pivot',
  have_hdmi: 'has_hdmi',
  have_displayport: 'has_displayport',
  have_serial: 'has_serial',
  have_parallel: 'has_parallel',
  have_usb: 'has_usb',
  have_ethernet: 'has_ethernet',
  have_wifi: 'has_wifi',
  have_headset: 'has_headset',
}

/** Name of a column: the label of its form field when there is one. */
export function useColumnLabel() {
  const { t } = useTranslation()
  return (column: string) => {
    const feature = FEATURE_OF_COLUMN[column]
    if (feature) {
      return t(`asset.features.${feature}`)
    }
    const field =
      FIELD_OF_COLUMN[column] ??
      (column.endsWith('types_id') ? 'type' : column.endsWith('models_id') ? 'model' : undefined)
    return field ? t(`asset.fields.${field}` as 'asset.fields.name') : column
  }
}

/** Why a write failed; a link to the classic interface when it must be made there. */
export function WriteError({ error, legacyPath }: { error: unknown; legacyPath: string }) {
  const { t } = useTranslation()
  const columnLabel = useColumnLabel()
  let message = t('asset.errors.generic')
  let legacy = false
  if (error instanceof ApiError) {
    if (error.code === 'legacy_required') {
      const reason = LEGACY_REASONS.find((known) => error.details.includes(known))
      message = `${t('asset.errors.legacy')} : ${reason ? t(`asset.errors.legacy_required.${reason}`) : ''}`
      legacy = true
    } else if (error.code === 'duplicate') {
      message = t('asset.errors.duplicate', { fields: error.details.map(columnLabel).join(', ') })
    } else if (error.code === 'sharing_in_use') {
      message = t('asset.errors.sharing_in_use')
    } else if (error.code === 'not_in_trash') {
      message = t('asset.errors.not_in_trash')
    } else if (error.code === 'position_unavailable') {
      message = t('asset.errors.position_unavailable')
    } else if (error.status === 403) {
      message = t('asset.errors.forbidden')
    } else if (error.status === 404) {
      message = t('asset.notFound')
    } else if (error.code === 'bad_request') {
      message = t('asset.errors.bad_request', { message: error.message })
    }
  }
  return (
    <div
      role="alert"
      className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200"
    >
      <p>{message}</p>
      {legacy && (
        <LegacyLink path={legacyPath} className="mt-2 inline-block font-medium underline">
          {t('asset.errors.openLegacy')} ↗
        </LegacyLink>
      )}
    </div>
  )
}
