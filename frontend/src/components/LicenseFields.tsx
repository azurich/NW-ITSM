import { useTranslation } from 'react-i18next'
import type { AssetType } from '../api/assets'
import type { AssetForm } from '../lib/assetForm'
import { ChoicePicker } from './ChoicePicker'
import { inputClass } from './PasswordFields'

/**
 * Fields of a license: its software (chosen at its creation), a parent
 * license and versions of the same software, its number, the over-quota
 * and its expiration.
 */
export function LicenseFields({
  kind,
  entityId,
  form,
  itemId,
  onChange,
}: {
  kind: AssetType
  entityId: number
  form: AssetForm
  /** The license being edited: its software is fixed, it is not its own parent. */
  itemId?: number
  onChange: (form: AssetForm) => void
}) {
  const { t } = useTranslation()
  const software = form.refs.software
  const filters = { software_id: software?.id, exclude_id: itemId }
  const hint = !software && (
    <span className="mt-1 block text-xs font-normal text-slate-500">{t('asset.license.chooseSoftware')}</span>
  )

  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <ChoicePicker
        kind={kind}
        field="software"
        entityId={entityId}
        label={t('asset.fields.software')}
        value={software}
        disabled={itemId !== undefined}
        // Another software: its licenses and versions are offered again.
        onChange={(value) =>
          onChange({
            ...form,
            refs: { ...form.refs, software: value, parent_license: null, version_use: null, version_buy: null },
          })
        }
      />
      <div>
        <ChoicePicker
          kind={kind}
          field="parent_license"
          entityId={entityId}
          label={t('asset.fields.parent_license')}
          value={form.refs.parent_license}
          disabled={!software}
          filters={filters}
          onChange={(value) => onChange({ ...form, refs: { ...form.refs, parent_license: value } })}
        />
        {hint}
      </div>
      {(['version_use', 'version_buy'] as const).map((field) => (
        <ChoicePicker
          key={field}
          kind={kind}
          field="license_version"
          entityId={entityId}
          label={t(`asset.fields.${field}`)}
          value={form.refs[field]}
          disabled={!software}
          filters={filters}
          onChange={(value) => onChange({ ...form, refs: { ...form.refs, [field]: value } })}
        />
      ))}
      <label className="block text-sm font-medium">
        {t('asset.fields.number')}
        <input
          type="number"
          step={1}
          min={0}
          max={10000}
          value={form.numbers.number}
          onChange={(event) => onChange({ ...form, numbers: { ...form.numbers, number: event.target.value } })}
          className={inputClass}
        />
      </label>
      <label className="block text-sm font-medium">
        {t('asset.fields.expire')}
        <input
          type="date"
          value={form.text.expire}
          onChange={(event) => onChange({ ...form, text: { ...form.text, expire: event.target.value } })}
          className={inputClass}
        />
      </label>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={form.flags.allow_overquota ?? false}
          onChange={(event) =>
            onChange({ ...form, flags: { ...form.flags, allow_overquota: event.target.checked } })
          }
        />
        {t('asset.fields.allow_overquota')}
      </label>
    </div>
  )
}
