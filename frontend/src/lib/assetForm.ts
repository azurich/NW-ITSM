import {
  ASSET_TYPES,
  type AssetDetail,
  type AssetType,
  type Choice,
  type Feature,
  type OptionalField,
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
  'version',
  'position',
  'background_color',
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
  'room',
] as const
export type RefField = (typeof REF_FIELDS)[number]

/** Whole numbers, typed as text. */
export const NUMBER_FIELDS = [
  'initial_page_counter',
  'page_counter',
  'room_orientation',
  'number_units',
  'width',
  'height',
  'depth',
  'max_power',
  'measured_power',
  'max_weight',
  'power_supplies',
] as const
export type NumberField = (typeof NUMBER_FIELDS)[number]

/** Sizes of a rack: empty is none; the other numbers are 0 when empty. */
const CLEARABLE_NUMBERS: readonly NumberField[] = [
  'width',
  'height',
  'depth',
  'max_power',
  'measured_power',
  'max_weight',
]

const RACK_FIELDS: readonly (TextField | RefField | NumberField)[] = [
  'room',
  'position',
  'room_orientation',
  'number_units',
  ...CLEARABLE_NUMBERS,
  'background_color',
]

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
  if (info.without?.includes(field as OptionalField)) {
    return false
  }
  if (RACK_FIELDS.includes(field)) {
    return !!info.rack
  }
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
    case 'power_supplies':
      return !!info.powerSupplies
    case 'version':
      return !!info.version
    default:
      return true
  }
}

/**
 * Form of a new item or template: empty, but the defaults of the
 * datacenter forms (42 units, a door to the north, a light red background,
 * one power supply).
 */
export function emptyForm(kind?: AssetType): AssetForm {
  const form: AssetForm = {
    text: Object.fromEntries(TEXT_FIELDS.map((field) => [field, ''])) as AssetForm['text'],
    refs: Object.fromEntries(REF_FIELDS.map((field) => [field, null])) as AssetForm['refs'],
    numbers: Object.fromEntries(NUMBER_FIELDS.map((field) => [field, ''])) as AssetForm['numbers'],
    flags: {},
    is_recursive: false,
  }
  if (kind && ASSET_TYPES[kind].rack) {
    form.numbers.number_units = '42'
    form.numbers.room_orientation = '1'
    form.text.background_color = '#f5b7b1'
  }
  if (kind && ASSET_TYPES[kind].powerSupplies) {
    form.numbers.power_supplies = '1'
  }
  return form
}

export function formFromDetail(asset: AssetDetail): AssetForm {
  const form = emptyForm()
  for (const field of TEXT_FIELDS) {
    form.text[field] = asset[field] ?? ''
  }
  // As the color input gives it.
  form.text.background_color = form.text.background_color.toLowerCase()
  for (const field of REF_FIELDS) {
    const ref = asset[field]
    form.refs[field] = ref ? { id: ref.id, label: ref.name ?? `#${ref.id}` } : null
  }
  for (const field of NUMBER_FIELDS) {
    form.numbers[field] = String(asset[field] ?? '')
  }
  // What the classic forms show for values they do not offer.
  const info = ASSET_TYPES[asset.kind]
  if (info.rack) {
    form.numbers.number_units = asset.number_units ? String(asset.number_units) : '42'
    if (!['1', '2', '3', '4'].includes(form.numbers.room_orientation)) {
      form.numbers.room_orientation = '1'
    }
  }
  if (info.powerSupplies && !asset.power_supplies) {
    form.numbers.power_supplies = '1'
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
      fields[field] = value !== '' ? Number(value) : CLEARABLE_NUMBERS.includes(field) ? null : 0
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
