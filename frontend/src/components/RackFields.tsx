import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { roomPositionsQuery, type AssetType } from '../api/assets'
import type { AssetForm, NumberField } from '../lib/assetForm'
import { ORIENTATIONS, parsePosition } from '../lib/rack'
import { ChoicePicker } from './ChoicePicker'
import { inputClass } from './PasswordFields'

const SIZES = ['width', 'height', 'depth', 'max_power', 'measured_power', 'max_weight'] as const

/**
 * Place and size of a rack: its server room (of its entity), a free place
 * of the room, the orientation of its door, its units, sizes and color.
 */
export function RackFields({
  kind,
  entityId,
  form,
  initial,
  onChange,
}: {
  kind: AssetType
  entityId: number
  form: AssetForm
  /** Stored values: the place of the rack in its room is offered. */
  initial: AssetForm
  onChange: (form: AssetForm) => void
}) {
  const { t } = useTranslation()
  const room = form.refs.room
  const current = room && room.id === initial.refs.room?.id ? initial.text.position : undefined
  const positions = useQuery({ ...roomPositionsQuery(room?.id ?? 0, current), enabled: !!room })
  const position = form.text.position
  const offered = positions.data ?? []
  const label = (value: string) => {
    const place = parsePosition(value)
    return place ? t('asset.rack.position', place) : value
  }

  const number = (field: NumberField, label: string, options: { min?: number; max?: number; required?: boolean } = {}) => (
    <label className="block text-sm font-medium">
      {label}
      <input
        type="number"
        step={1}
        {...options}
        value={form.numbers[field]}
        onChange={(event) => onChange({ ...form, numbers: { ...form.numbers, [field]: event.target.value } })}
        className={inputClass}
      />
    </label>
  )

  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <ChoicePicker
        kind={kind}
        field="room"
        entityId={entityId}
        label={t('asset.fields.room')}
        value={room}
        // Another room: its places are offered again.
        onChange={(value) =>
          onChange({
            ...form,
            refs: { ...form.refs, room: value },
            text: { ...form.text, position: value?.id === initial.refs.room?.id ? initial.text.position : '0' },
          })
        }
      />
      <label className="block text-sm font-medium">
        {t('asset.fields.position')}
        <select
          value={parsePosition(position) ? position : '0'}
          disabled={!room || positions.isPending}
          onChange={(event) => onChange({ ...form, text: { ...form.text, position: event.target.value } })}
          className={inputClass}
        >
          <option value="0">{t('asset.form.none')}</option>
          {parsePosition(position) && !offered.some((place) => place.position === position) && (
            <option value={position}>{label(position)}</option>
          )}
          {offered.map((place) => (
            <option key={place.position} value={place.position}>
              {t('asset.rack.position', { column: place.column, row: place.row })}
            </option>
          ))}
        </select>
        {!room && <span className="mt-1 block text-xs font-normal text-slate-500">{t('asset.rack.chooseRoom')}</span>}
      </label>
      <label className="block text-sm font-medium">
        {t('asset.fields.room_orientation')}
        <select
          value={form.numbers.room_orientation}
          onChange={(event) =>
            onChange({ ...form, numbers: { ...form.numbers, room_orientation: event.target.value } })
          }
          className={inputClass}
        >
          {Object.entries(ORIENTATIONS).map(([value, name]) => (
            <option key={value} value={value}>
              {t(`asset.orientations.${name}`)}
            </option>
          ))}
        </select>
      </label>
      {number('number_units', t('asset.fields.number_units'), { min: 1, max: 100, required: true })}
      {SIZES.map((field) => (
        <div key={field}>{number(field, t(`asset.fields.${field}`), { min: 0 })}</div>
      ))}
      <label className="block text-sm font-medium">
        {t('asset.fields.background_color')}
        <input
          type="color"
          value={form.text.background_color || '#000000'}
          onChange={(event) =>
            onChange({ ...form, text: { ...form.text, background_color: event.target.value } })
          }
          className="mt-1 block h-9 w-20 cursor-pointer rounded-md border border-slate-300 bg-white p-1 dark:border-slate-700 dark:bg-slate-900"
        />
      </label>
    </div>
  )
}
