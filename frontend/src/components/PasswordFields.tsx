import { useTranslation } from 'react-i18next'
import type { PasswordErrorKey } from '../lib/passwordErrors'

export const inputClass =
  'mt-1 block w-full rounded-md border border-slate-300 bg-white px-3 py-2 shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/30 dark:border-slate-700 dark:bg-slate-900'

/** New password and its confirmation. */
export function NewPasswordFields({
  value,
  confirmation,
  onChange,
  onConfirmationChange,
}: {
  value: string
  confirmation: string
  onChange: (value: string) => void
  onConfirmationChange: (value: string) => void
}) {
  const { t } = useTranslation()
  return (
    <>
      <label className="block text-sm font-medium">
        {t('password.new')}
        <input
          name="new-password"
          type="password"
          autoComplete="new-password"
          required
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className={inputClass}
        />
      </label>
      <label className="block text-sm font-medium">
        {t('password.confirm')}
        <input
          name="new-password-confirmation"
          type="password"
          autoComplete="new-password"
          required
          value={confirmation}
          onChange={(event) => onConfirmationChange(event.target.value)}
          className={inputClass}
        />
      </label>
    </>
  )
}

export function PasswordErrors({ keys }: { keys: PasswordErrorKey[] }) {
  const { t } = useTranslation()
  if (keys.length === 0) {
    return null
  }
  return (
    <ul role="alert" className="space-y-1 rounded-md bg-red-50 p-3 text-sm text-red-800 dark:bg-red-950 dark:text-red-200">
      {keys.map((key) => (
        <li key={key}>{t(`password.errors.${key}`)}</li>
      ))}
    </ul>
  )
}
