import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import { Link, useParams } from '@tanstack/react-router'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import { ASSET_TYPES, purgeTemplate, templatesQuery, type TemplateSummary } from '../api/assets'
import { PURGE, hasRight, sessionQuery } from '../api/session'
import { WriteError } from '../components/AssetForm'
import { LegacyLink } from '../components/LegacyLink'
import { ErrorState, Loading } from '../components/States'
import { BackLink } from './AssetPage'

/** Templates of a type (`CommonDBTM::listTemplates`): to change or delete them. */
export function TemplatesPage() {
  const { t } = useTranslation()
  const { kind } = useParams({ from: '/authenticated/assets/$kind/templates/' })
  const { data: session } = useSuspenseQuery(sessionQuery)
  const query = useQuery(templatesQuery(kind))
  const queryClient = useQueryClient()
  const [confirm, setConfirm] = useState<TemplateSummary | null>(null)
  const purge = useMutation({
    mutationFn: (id: number) => purgeTemplate(kind, id),
    onSuccess: async () => {
      setConfirm(null)
      await queryClient.invalidateQueries({ queryKey: ['templates', kind] })
    },
  })

  const info = ASSET_TYPES[kind]
  const header = (
    <div className="space-y-2">
      <BackLink kind={kind} />
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-2xl font-semibold">{t('templates.title', { type: t(`assets.types.${kind}`) })}</h1>
        {query.data?.can_create && (
          <Link
            to="/assets/$kind/templates/new"
            params={{ kind }}
            className="rounded-md bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
          >
            {t('templates.new')}
          </Link>
        )}
        <LegacyLink
          path={`front/setup.templates.php?itemtype=${info.legacy}&add=0`}
          className="ml-auto text-sm text-slate-500 hover:underline"
        >
          {t('asset.openLegacy')} ↗
        </LegacyLink>
      </div>
    </div>
  )
  if (query.error instanceof ApiError && query.error.status === 403) {
    return (
      <div className="space-y-4">
        {header}
        <ErrorState message={t('assets.forbidden')} />
      </div>
    )
  }
  if (query.isPending) {
    return <Loading />
  }
  if (query.isError) {
    return <ErrorState onRetry={() => void query.refetch()} />
  }

  const { items, can_create: canCreate } = query.data
  const canPurge = hasRight(session, info.right, PURGE)
  const label = (template: TemplateSummary) => template.template_name || t('templates.unnamed', { id: template.id })
  return (
    <section className="space-y-4">
      {header}
      {confirm && (
        <div
          role="alertdialog"
          aria-label={t('asset.purge')}
          className="flex flex-wrap items-center gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100"
        >
          <p>{t('templates.purgeConfirm', { name: label(confirm) })}</p>
          <button
            type="button"
            disabled={purge.isPending}
            onClick={() => purge.mutate(confirm.id)}
            className="rounded-md bg-red-600 px-3 py-1.5 font-medium text-white hover:bg-red-500 disabled:opacity-60"
          >
            {t('asset.purgeConfirmButton')}
          </button>
          <button
            type="button"
            onClick={() => {
              setConfirm(null)
              purge.reset()
            }}
            className="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          >
            {t('asset.form.cancel')}
          </button>
        </div>
      )}
      {purge.isError && confirm && (
        <WriteError
          error={purge.error}
          legacyPath={`front/${info.legacy}.form.php?id=${confirm.id}&withtemplate=1`}
        />
      )}
      <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900">
        <table className="min-w-full text-left text-sm">
          <thead className="border-b border-slate-200 bg-slate-50 text-xs uppercase text-slate-500 dark:border-slate-800 dark:bg-slate-900/60">
            <tr>
              <th scope="col" className="px-3 py-2 font-medium">
                {t('templates.name')}
              </th>
              <th scope="col" className="px-3 py-2 font-medium">
                {t('assets.columns.entity')}
              </th>
              <th scope="col" className="px-3 py-2 font-medium">
                {t('templates.itemName')}
              </th>
              <th scope="col" className="px-3 py-2">
                <span className="sr-only">{t('templates.actions')}</span>
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100 dark:divide-slate-800">
            {items.length === 0 ? (
              <tr>
                <td colSpan={4} className="px-3 py-8 text-center text-slate-500">
                  {t('templates.empty')}
                </td>
              </tr>
            ) : (
              items.map((template) => (
                <tr key={template.id} className="hover:bg-slate-50 dark:hover:bg-slate-800/50">
                  <td className="whitespace-nowrap px-3 py-2">
                    {canCreate ? (
                      <Link
                        to="/assets/$kind/templates/$id/edit"
                        params={{ kind, id: template.id }}
                        className="font-medium text-indigo-600 hover:underline dark:text-indigo-400"
                      >
                        {label(template)}
                      </Link>
                    ) : (
                      label(template)
                    )}
                  </td>
                  <td className="px-3 py-2">{template.entity.name}</td>
                  <td className="px-3 py-2">{template.name}</td>
                  <td className="whitespace-nowrap px-3 py-2 text-right">
                    {canCreate && (
                      <Link
                        to="/assets/$kind/new"
                        params={{ kind }}
                        search={{ template: template.id }}
                        className="mr-3 text-indigo-600 hover:underline dark:text-indigo-400"
                      >
                        {t('templates.use')}
                      </Link>
                    )}
                    {canCreate && canPurge && (
                      <button
                        type="button"
                        onClick={() => {
                          purge.reset()
                          setConfirm(template)
                        }}
                        className="text-red-700 hover:underline dark:text-red-300"
                      >
                        {t('asset.purge')}
                      </button>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  )
}
