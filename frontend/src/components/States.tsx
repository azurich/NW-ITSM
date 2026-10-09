import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

export function Loading() {
  const { t } = useTranslation()
  return (
    <p role="status" className="py-12 text-center text-sm text-slate-500">
      {t('app.loading')}
    </p>
  )
}

export function ErrorState({ message, onRetry }: { message?: string; onRetry?: () => void }) {
  const { t } = useTranslation()
  return (
    <div
      role="alert"
      className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200"
    >
      <p>{message ?? t('errors.generic')}</p>
      {onRetry && (
        <button type="button" onClick={onRetry} className="mt-2 font-medium underline">
          {t('errors.retry')}
        </button>
      )}
    </div>
  )
}

export function NotFound() {
  const { t } = useTranslation()
  return (
    <div className="mx-auto max-w-md py-24 text-center">
      <p className="text-lg">{t('app.notFound')}</p>
      <Link to="/" className="mt-4 inline-block text-indigo-600 hover:underline dark:text-indigo-400">
        {t('app.home')}
      </Link>
    </div>
  )
}
