import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import { login, sessionQuery } from '../api/session'
import { applyUserLanguage } from '../i18n'

const KNOWN_ERRORS = ['invalid_credentials', 'account_disabled', 'password_expired', 'no_profile'] as const

export function LoginPage() {
  const { t } = useTranslation()
  const { redirect } = useSearch({ from: '/login' })
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [credentials, setCredentials] = useState({ login: '', password: '' })

  const mutation = useMutation({
    mutationFn: () => login(credentials.login, credentials.password),
    onSuccess: (session) => {
      queryClient.setQueryData(sessionQuery.queryKey, session)
      applyUserLanguage(session.user.language)
      void navigate({ to: redirect ?? '/' })
    },
  })

  const errorCode =
    mutation.error instanceof ApiError
      ? (KNOWN_ERRORS.find((code) => code === (mutation.error as ApiError).code) ?? 'unknown')
      : mutation.error
        ? 'unknown'
        : null

  const submit = (event: FormEvent) => {
    event.preventDefault()
    mutation.mutate()
  }

  const inputClass =
    'mt-1 block w-full rounded-md border border-slate-300 bg-white px-3 py-2 shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/30 dark:border-slate-700 dark:bg-slate-900'

  return (
    <div className="flex min-h-screen items-center justify-center px-4">
      <form
        onSubmit={submit}
        className="w-full max-w-sm space-y-5 rounded-xl border border-slate-200 bg-white p-8 shadow-sm dark:border-slate-800 dark:bg-slate-900"
      >
        <div>
          <p className="text-sm font-semibold text-indigo-600 dark:text-indigo-400">{t('app.name')}</p>
          <h1 className="text-2xl font-semibold">{t('login.title')}</h1>
        </div>
        {errorCode && (
          <p role="alert" className="rounded-md bg-red-50 p-3 text-sm text-red-800 dark:bg-red-950 dark:text-red-200">
            {t(`login.errors.${errorCode}`)}
          </p>
        )}
        <label className="block text-sm font-medium">
          {t('login.login')}
          <input
            name="login"
            autoComplete="username"
            required
            autoFocus
            value={credentials.login}
            onChange={(event) => setCredentials({ ...credentials, login: event.target.value })}
            className={inputClass}
          />
        </label>
        <label className="block text-sm font-medium">
          {t('login.password')}
          <input
            name="password"
            type="password"
            autoComplete="current-password"
            required
            value={credentials.password}
            onChange={(event) => setCredentials({ ...credentials, password: event.target.value })}
            className={inputClass}
          />
        </label>
        <button
          type="submit"
          disabled={mutation.isPending}
          className="w-full rounded-md bg-indigo-600 px-4 py-2 font-medium text-white hover:bg-indigo-500 disabled:opacity-60"
        >
          {mutation.isPending ? t('login.submitting') : t('login.submit')}
        </button>
        <p className="text-xs text-slate-500">{t('login.localOnly')}</p>
      </form>
    </div>
  )
}
