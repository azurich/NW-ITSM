import { useMutation } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import { login, replaceExpiredPassword, type SessionInfo } from '../api/session'
import { NewPasswordFields, PasswordErrors, inputClass } from '../components/PasswordFields'
import { useApplySession } from '../hooks/useApplySession'
import { passwordErrorKeys, type PasswordErrorKey } from '../lib/passwordErrors'

const KNOWN_ERRORS = ['invalid_credentials', 'account_disabled', 'no_profile', 'too_many_attempts'] as const

const buttonClass =
  'w-full rounded-md bg-indigo-600 px-4 py-2 font-medium text-white hover:bg-indigo-500 disabled:opacity-60'

export function LoginPage() {
  const { t } = useTranslation()
  const { redirect, bridge } = useSearch({ from: '/login' })
  const navigate = useNavigate()
  const applySession = useApplySession()
  const [credentials, setCredentials] = useState({ login: '', password: '' })
  const [expired, setExpired] = useState(false)

  const enter = async (session: SessionInfo) => {
    await applySession(session)
    void navigate({ to: redirect ?? '/' })
  }

  const mutation = useMutation({
    mutationFn: () => login(credentials.login, credentials.password),
    onSuccess: enter,
    onError: (error) => setExpired(error instanceof ApiError && error.code === 'password_expired'),
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    mutation.mutate()
  }

  const errorCode =
    mutation.error instanceof ApiError
      ? (KNOWN_ERRORS.find((code) => code === (mutation.error as ApiError).code) ?? 'unknown')
      : mutation.error
        ? 'unknown'
        : null

  return (
    <div className="flex min-h-screen items-center justify-center px-4">
      <div className="w-full max-w-sm space-y-5 rounded-xl border border-slate-200 bg-white p-8 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <p className="text-sm font-semibold text-indigo-600 dark:text-indigo-400">{t('app.name')}</p>
        {expired ? (
          <ExpiredPasswordForm
            login={credentials.login}
            currentPassword={credentials.password}
            onBack={() => {
              setExpired(false)
              mutation.reset()
            }}
            onDone={enter}
          />
        ) : (
          <form onSubmit={submit} className="space-y-5">
            <h1 className="text-2xl font-semibold">{t('login.title')}</h1>
            {bridge === 'failed' && !errorCode && (
              <p role="status" className="rounded-md bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
                {t('login.bridgeFailed')}
              </p>
            )}
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
            <button type="submit" disabled={mutation.isPending} className={buttonClass}>
              {mutation.isPending ? t('login.submitting') : t('login.submit')}
            </button>
            <p className="text-xs text-slate-500">{t('login.localOnly')}</p>
          </form>
        )}
      </div>
    </div>
  )
}

function ExpiredPasswordForm({
  login,
  currentPassword,
  onBack,
  onDone,
}: {
  login: string
  currentPassword: string
  onBack: () => void
  onDone: (session: SessionInfo) => Promise<void>
}) {
  const { t } = useTranslation()
  const [next, setNext] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const [mismatch, setMismatch] = useState(false)
  const mutation = useMutation({
    mutationFn: () => replaceExpiredPassword(login, currentPassword, next),
    onSuccess: onDone,
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setMismatch(next !== confirmation)
    if (next === confirmation) {
      mutation.mutate()
    }
  }
  const errors: PasswordErrorKey[] = mismatch ? ['mismatch'] : mutation.isError ? passwordErrorKeys(mutation.error) : []

  return (
    <form onSubmit={submit} className="space-y-5">
      <h1 className="text-2xl font-semibold">{t('login.expiredTitle')}</h1>
      <p className="text-sm text-slate-600 dark:text-slate-400">{t('login.expiredHelp')}</p>
      <PasswordErrors keys={errors} />
      <NewPasswordFields value={next} confirmation={confirmation} onChange={setNext} onConfirmationChange={setConfirmation} />
      <button type="submit" disabled={mutation.isPending} className={buttonClass}>
        {t('login.replace')}
      </button>
      <button type="button" onClick={onBack} className="w-full text-sm text-slate-500 hover:underline">
        {t('login.back')}
      </button>
    </form>
  )
}
