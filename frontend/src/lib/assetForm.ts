import { ASSET_TYPES, type AssetDetail, type AssetFields, type AssetType, type Choice } from '../api/assets'

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
] as const
export type RefField = (typeof REF_FIELDS)[number]

export interface AssetForm {
  text: Record<TextField, string>
  refs: Record<RefField, Choice | null>
  is_recursive: boolean
}

/** Whether the type has the field. */
export function hasField(kind: AssetType, field: TextField | RefField): boolean {
  const info = ASSET_TYPES[kind]
  switch (field) {
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
  form.is_recursive = asset.is_recursive
  return form
}

/**
 * Fields to send: those of the type that differ from `initial`, or every
 * filled one for a creation.
 */
export function changedFields(kind: AssetType, form: AssetForm, initial?: AssetForm): AssetFields {
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
  if (form.is_recursive !== (initial?.is_recursive ?? false)) {
    fields.is_recursive = form.is_recursive
  }
  return fields as AssetFields
}
