import {
  ASSET_TYPES,
  type AssetDetail,
  type AssetType,
  type Choice,
  type Feature,
  type TemplateFields,
} from '../api/assets'

export const TEXT_FIELDS = [
  'name',
  'serial',
  'inventory_number',
  'uuid',
  'brand',
  'number_line',
  'contact',
  'contact_number',
  'comment',
  'size',
  'memory',
] as const
export type TextField = (typeof TEXT_FIELDS)[number]

/** Fields chosen in a list; the API takes them as `<field>_id`. */
export const REF_FIELDS = [
  'type',
  'manufacturer',
  'model',
  'network',
  'update_source',
  'state',
  'location',
  'user',
  'group',
  'tech_user',
  'tech_group',
  'power_supply',
] as const
export type RefField = (typeof REF_FIELDS)[number]

/** Whole numbers, typed as text. */
export const NUMBER_FIELDS = ['initial_page_counter', 'page_counter'] as const
export type NumberField = (typeof NUMBER_FIELDS)[number]

/** Switches: the management type and the features. */
export type FlagField = Feature | 'is_global'

export interface AssetForm {
  text: Record<TextField, string>
  refs: Record<RefField, Choice | null>
  numbers: Record<NumberField, string>
  flags: Partial<Record<FlagField, boolean>>
  is_recursive: boolean
  /** Name of a template; absent for an item. */
  template_name?: string
}

/** Switches of the type. */
export function flagFields(kind: AssetType): FlagField[] {
  const info = ASSET_TYPES[kind]
  return [...(info.global ? (['is_global'] as const) : []), ...(info.features ?? [])]
}

/** Whether the type has the field. */
export function hasField(kind: AssetType, field: TextField | RefField | NumberField): boolean {
  const info = ASSET_TYPES[kind]
  switch (field) {
    case 'size':
      return !!info.size
    case 'memory':
      return !!info.memory
    case 'power_supply':
      return !!info.powerSupply
    case 'initial_page_counter':
    case 'page_counter':
      return !!info.pageCounters
    case 'uuid':
      return !!info.uuid
    case 'brand':
      return !!info.brand
    case 'number_line':
      return !!info.numberLine
    case 'network':
      return !!info.network
    case 'update_source':
      return !!info.updateSource
    default:
      return true
  }
}

export function emptyForm(): AssetForm {
  return {
    text: Object.fromEntries(TEXT_FIELDS.map((field) => [field, ''])) as AssetForm['text'],
    refs: Object.fromEntries(REF_FIELDS.map((field) => [field, null])) as AssetForm['refs'],
    numbers: { initial_page_counter: '', page_counter: '' },
    flags: {},
    is_recursive: false,
  }
}

export function formFromDetail(asset: AssetDetail): AssetForm {
  const form = emptyForm()
  for (const field of TEXT_FIELDS) {
    form.text[field] = asset[field] ?? ''
  }
  for (const field of REF_FIELDS) {
    const ref = asset[field]
    form.refs[field] = ref ? { id: ref.id, label: ref.name ?? `#${ref.id}` } : null
  }
  for (const field of NUMBER_FIELDS) {
    form.numbers[field] = String(asset[field] ?? '')
  }
  for (const field of flagFields(asset.kind)) {
    form.flags[field] = asset[field] ?? false
  }
  form.is_recursive = asset.is_recursive
  if (asset.is_template) {
    form.template_name = asset.template_name ?? ''
  }
  return form
}

/**
 * Fields to send: those of the type that differ from `initial`, or every
 * filled one for a creation.
 */
export function changedFields(kind: AssetType, form: AssetForm, initial?: AssetForm): TemplateFields {
  const fields: Record<string, string | number | boolean | null> = {}
  for (const field of TEXT_FIELDS) {
    const value = form.text[field]
    if (hasField(kind, field) && (initial ? value !== initial.text[field] : value !== '')) {
      fields[field] = value
    }
  }
  for (const field of REF_FIELDS) {
    const id = form.refs[field]?.id ?? null
    const before = initial ? (initial.refs[field]?.id ?? null) : null
    if (hasField(kind, field) && id !== before) {
      fields[`${field}_id`] = id
    }
  }
  for (const field of NUMBER_FIELDS) {
    const value = form.numbers[field].trim()
    if (hasField(kind, field) && (initial ? value !== initial.numbers[field] : value !== '')) {
      // Like the classic interface, an empty counter is 0.
      fields[field] = value === '' ? 0 : Number(value)
    }
  }
  for (const field of flagFields(kind)) {
    const value = form.flags[field] ?? false
    if (value !== (initial?.flags[field] ?? false)) {
      fields[field] = value
    }
  }
  if (form.is_recursive !== (initial?.is_recursive ?? false)) {
    fields.is_recursive = form.is_recursive
  }
  if (form.template_name !== undefined && (initial ? form.template_name !== initial.template_name : true)) {
    fields.template_name = form.template_name
  }
  return fields as TemplateFields
}
