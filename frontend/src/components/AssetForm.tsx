import { useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import type { AssetType, ChoiceField } from '../api/assets'
import { hasField, type AssetForm, type RefField, type TextField } from '../lib/assetForm'
import { ChoicePicker } from './ChoicePicker'
import { LegacyLink } from './LegacyLink'
import { inputClass } from './PasswordFields'

const sectionClass = 'rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900'

/** Form of an asset, for a creation or an update. */
export function AssetFormView({
  kind,
  entityId,
  entityField,
  initial,
  recursiveLocked,
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
  /** Already shared with the sub-entities: only the classic interface unshares. */
  recursiveLocked?: boolean
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

  const text = (field: TextField) =>
    hasField(kind, field) && (
      <label className="block text-sm font-medium">
        {t(`asset.fields.${field}`)}
        <input
          type="text"
          value={form.text[field]}
          maxLength={255}
          onChange={(event) => setForm({ ...form, text: { ...form.text, [field]: event.target.value } })}
          className={inputClass}
        />
      </label>
    )
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
                disabled={recursiveLocked}
                onChange={(event) => setForm({ ...form, is_recursive: event.target.checked })}
              />
              {t('asset.fields.is_recursive')}
            </label>
            {recursiveLocked && <p className="mt-1 text-xs text-slate-500">{t('asset.form.recursiveLocked')}</p>}
          </div>
        </fieldset>
      </div>

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
  'infocom_creation',
  'infocom_dates',
  'plugins',
  'unrecursive',
] as const

/** Why a write failed; a link to the classic interface when it must be made there. */
export function WriteError({ error, legacyPath }: { error: unknown; legacyPath: string }) {
  const { t } = useTranslation()
  let message = t('asset.errors.generic')
  let legacy = false
  if (error instanceof ApiError) {
    if (error.code === 'legacy_required') {
      const reason = LEGACY_REASONS.find((known) => error.details.includes(known))
      message = `${t('asset.errors.legacy')} : ${reason ? t(`asset.errors.legacy_required.${reason}`) : ''}`
      legacy = true
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
