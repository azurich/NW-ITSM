import { useMutation, useQuery, useSuspenseQuery } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { changePassword, languagesQuery, sessionQuery, updateLanguage } from '../api/session'
import { NewPasswordFields, PasswordErrors, inputClass } from '../components/PasswordFields'
import { useApplySession } from '../hooks/useApplySession'
import { passwordErrorKeys, type PasswordErrorKey } from '../lib/passwordErrors'

const sectionClass =
  'space-y-4 rounded-lg border border-slate-200 bg-white p-6 dark:border-slate-800 dark:bg-slate-900'

export function PreferencesPage() {
  const { t } = useTranslation()
  return (
    <div className="max-w-xl space-y-6">
      <h1 className="text-2xl font-semibold">{t('preferences.title')}</h1>
      <LanguageSection />
      <PasswordSection />
    </div>
  )
}

function LanguageSection() {
  const { t } = useTranslation()
  const { data: session } = useSuspenseQuery(sessionQuery)
  const languages = useQuery(languagesQuery)
  const applySession = useApplySession()
  const mutation = useMutation({ mutationFn: updateLanguage, onSuccess: applySession })

  const current = session.user.language_is_default ? '' : session.user.language
  const nameOf = (code: string) => languages.data?.find((language) => language.code === code)?.name ?? code

  return (
    <section className={sectionClass}>
      <label className="block text-sm font-medium">
        {t('preferences.language')}
        <select
          value={current}
          disabled={mutation.isPending || languages.isPending}
          onChange={(event) => mutation.mutate(event.target.value || null)}
          className={inputClass}
        >
          <option value="">
            {t('preferences.defaultLanguage', {
              language: session.user.language_is_default ? nameOf(session.user.language) : '…',
            })}
          </option>
          {languages.data?.map((language) => (
            <option key={language.code} value={language.code}>
              {language.name}
            </option>
          ))}
        </select>
      </label>
      {mutation.isSuccess && (
        <p role="status" className="text-sm text-green-700 dark:text-green-400">
          {t('preferences.saved')}
        </p>
      )}
    </section>
  )
}

function PasswordSection() {
  const { t } = useTranslation()
  const { data: session } = useSuspenseQuery(sessionQuery)
  const applySession = useApplySession()
  const [form, setForm] = useState({ current: '', next: '', confirmation: '' })
  const [mismatch, setMismatch] = useState(false)
  const mutation = useMutation({
    mutationFn: () => changePassword(form.current, form.next),
    onSuccess: async (updated) => {
      setForm({ current: '', next: '', confirmation: '' })
      await applySession(updated)
    },
  })

  if (!session.capabilities.change_password) {
    return (
      <section className={sectionClass}>
        <h2 className="text-lg font-semibold">{t('password.title')}</h2>
        <p className="text-sm text-slate-600 dark:text-slate-400">{t('password.notAllowed')}</p>
      </section>
    )
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const differs = form.next !== form.confirmation
    setMismatch(differs)
    if (!differs) {
      mutation.mutate()
    }
  }
  const errors: PasswordErrorKey[] = mismatch
    ? ['mismatch']
    : mutation.isError
      ? passwordErrorKeys(mutation.error)
      : []

  return (
    <form onSubmit={submit} className={sectionClass}>
      <h2 className="text-lg font-semibold">{t('password.title')}</h2>
      <PasswordErrors keys={errors} />
      {mutation.isSuccess && (
        <p role="status" className="text-sm text-green-700 dark:text-green-400">
          {t('password.changed')}
        </p>
      )}
      <label className="block text-sm font-medium">
        {t('password.current')}
        <input
          name="current-password"
          type="password"
          autoComplete="current-password"
          required
          value={form.current}
          onChange={(event) => setForm({ ...form, current: event.target.value })}
          className={inputClass}
        />
      </label>
      <NewPasswordFields
        value={form.next}
        confirmation={form.confirmation}
        onChange={(next) => setForm({ ...form, next })}
        onConfirmationChange={(confirmation) => setForm({ ...form, confirmation })}
      />
      <button
        type="submit"
        disabled={mutation.isPending}
        className="rounded-md bg-indigo-600 px-4 py-2 font-medium text-white hover:bg-indigo-500 disabled:opacity-60"
      >
        {t('password.submit')}
      </button>
    </form>
  )
}
