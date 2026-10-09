import { useQuery } from '@tanstack/react-query'
import { Link, useParams } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '../api/client'
import { computerQuery } from '../api/computers'
import { ErrorState, Loading } from '../components/States'
import { formatDateTime, legacyUrl } from '../lib/format'

export function ComputerPage() {
  const { t, i18n } = useTranslation()
  const { computerId } = useParams({ from: '/authenticated/assets/computers/$computerId' })
  const query = useQuery({ ...computerQuery(computerId), enabled: Number.isInteger(computerId) })

  const back = (
    <Link to="/assets/computers" className="text-sm text-indigo-600 hover:underline dark:text-indigo-400">
      ← {t('computer.back')}
    </Link>
  )

  if (!Number.isInteger(computerId) || (query.error instanceof ApiError && query.error.status === 404)) {
    return (
      <div className="space-y-4">
        {back}
        <ErrorState message={t('computer.notFound')} />
      </div>
    )
  }
  if (query.isPending) {
    return <Loading />
  }
  if (query.isError) {
    return <ErrorState onRetry={() => void query.refetch()} />
  }

  const computer = query.data
  const date = (value: string | null | undefined) => formatDateTime(value, i18n.language)

  return (
    <article className="space-y-6">
      <div className="space-y-2">
        {back}
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-2xl font-semibold">{computer.name || t('computers.unnamed')}</h1>
          {computer.is_deleted && <Badge tone="red">{t('computer.trash')}</Badge>}
          {computer.is_dynamic && <Badge tone="blue">{t('computer.dynamic')}</Badge>}
          {computer.is_recursive && <Badge tone="slate">{t('computer.recursive')}</Badge>}
          <a
            href={legacyUrl(`front/computer.form.php?id=${computer.id}`)}
            className="ml-auto text-sm text-slate-500 hover:underline"
          >
            {t('computer.openLegacy')} ↗
          </a>
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <Section title={t('computer.sections.general')}>
          <Field label={t('computer.fields.entity')}>{computer.entity.name}</Field>
          <Field label={t('computer.fields.type')}>{computer.type?.name}</Field>
          <Field label={t('computer.fields.manufacturer')}>{computer.manufacturer?.name}</Field>
          <Field label={t('computer.fields.model')}>{computer.model?.name}</Field>
          <Field label={t('computer.fields.serial')}>{computer.serial}</Field>
          <Field label={t('computer.fields.inventory_number')}>{computer.inventory_number}</Field>
          <Field label={t('computer.fields.uuid')}>{computer.uuid}</Field>
          <Field label={t('computer.fields.created_at')}>{date(computer.created_at)}</Field>
          <Field label={t('computer.fields.updated_at')}>{date(computer.updated_at)}</Field>
        </Section>
        <Section title={t('computer.sections.management')}>
          <Field label={t('computer.fields.state')}>{computer.state?.name}</Field>
          <Field label={t('computer.fields.location')}>{computer.location?.name}</Field>
          <Field label={t('computer.fields.user')}>{computer.user?.name}</Field>
          <Field label={t('computer.fields.group')}>{computer.group?.name}</Field>
          <Field label={t('computer.fields.contact')}>{computer.contact}</Field>
          <Field label={t('computer.fields.contact_number')}>{computer.contact_number}</Field>
          <Field label={t('computer.fields.tech_user')}>{computer.tech_user?.name}</Field>
          <Field label={t('computer.fields.tech_group')}>{computer.tech_group?.name}</Field>
        </Section>
      </div>

      {computer.comment && (
        <Section title={t('computer.sections.notes')}>
          <p className="whitespace-pre-wrap text-sm">{computer.comment}</p>
        </Section>
      )}
    </article>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
      <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-slate-500">{title}</h2>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 text-sm">{children}</dl>
    </section>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-slate-500">{label}</dt>
      <dd className="break-words">{children || '—'}</dd>
    </>
  )
}

const TONES = {
  red: 'bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-200',
  blue: 'bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-200',
  slate: 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-200',
}

function Badge({ tone, children }: { tone: keyof typeof TONES; children: ReactNode }) {
  return <span className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${TONES[tone]}`}>{children}</span>
}
