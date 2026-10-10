import { keepPreviousData, queryOptions } from '@tanstack/react-query'
import { api, unwrap, type Schemas } from './client'
import type { paths } from './schema'

export type AssetType = Schemas['AssetType']
export type AssetPage = Schemas['AssetPage']
export type AssetSummary = Schemas['AssetSummary']
export type AssetDetail = Schemas['AssetDetail']
export type AssetFields = Schemas['AssetFields']
export type ChoiceField = Schemas['ChoiceField']
export type Choice = Schemas['Choice']
export type DuplicateNotice = Schemas['DuplicateNotice']
/** Connector or feature of a type (`has_*`). */
export type Feature = keyof Schemas['Features']

type ListQuery = NonNullable<paths['/api/v2/assets/{kind}']['get']['parameters']['query']>
export type AssetSort = NonNullable<ListQuery['sort']>
export type SortOrder = NonNullable<ListQuery['order']>

/** What differs between the asset types. */
export interface AssetTypeInfo {
  /** Right module of the legacy profiles. */
  right: string
  /** Page of the classic interface: `front/<legacy>.php`, `front/<legacy>.form.php`. */
  legacy: string
  uuid?: boolean
  brand?: boolean
  numberLine?: boolean
  network?: boolean
  updateSource?: boolean
  /** Can be used by several computers at once. */
  global?: boolean
  /** Diagonal of the screen. */
  size?: boolean
  /** Memory: free text, or a number of MiB. */
  memory?: 'text' | 'mib'
  /** Initial and current page counters. */
  pageCounters?: boolean
  powerSupply?: boolean
  /** Connectors and features, in the order of the classic interface. */
  features?: readonly Feature[]
}

export const ASSET_TYPES: Record<AssetType, AssetTypeInfo> = {
  computers: { right: 'computer', legacy: 'computer', uuid: true, network: true, updateSource: true },
  monitors: {
    right: 'monitor',
    legacy: 'monitor',
    global: true,
    size: true,
    features: [
      'has_microphone',
      'has_speaker',
      'has_subd',
      'has_bnc',
      'has_dvi',
      'has_pivot',
      'has_hdmi',
      'has_displayport',
    ],
  },
  printers: {
    right: 'printer',
    legacy: 'printer',
    network: true,
    global: true,
    memory: 'text',
    pageCounters: true,
    features: ['has_serial', 'has_parallel', 'has_usb', 'has_ethernet', 'has_wifi'],
  },
  'network-equipments': { right: 'networking', legacy: 'networkequipment', network: true, memory: 'mib' },
  peripherals: { right: 'peripheral', legacy: 'peripheral', brand: true, global: true },
  phones: {
    right: 'phone',
    legacy: 'phone',
    brand: true,
    numberLine: true,
    global: true,
    powerSupply: true,
    features: ['has_headset', 'has_speaker'],
  },
}

export const ASSET_TYPE_LIST = Object.keys(ASSET_TYPES) as AssetType[]

export function isAssetType(value: unknown): value is AssetType {
  return typeof value === 'string' && Object.hasOwn(ASSET_TYPES, value)
}

export const PER_PAGE = 20

export interface AssetListParams {
  page: number
  search: string
  sort: AssetSort
  order: SortOrder
  trash: boolean
}

export const assetsQuery = (kind: AssetType, params: AssetListParams) =>
  queryOptions({
    queryKey: ['assets', kind, params],
    queryFn: async () =>
      unwrap(
        await api.GET('/api/v2/assets/{kind}', {
          params: {
            path: { kind },
            query: {
              page: params.page,
              per_page: PER_PAGE,
              search: params.search || undefined,
              sort: params.sort,
              order: params.order,
              trash: params.trash || undefined,
            },
          },
        }),
      ),
    placeholderData: keepPreviousData,
  })

export const assetQuery = (kind: AssetType, id: number) =>
  queryOptions({
    queryKey: ['asset', kind, id],
    queryFn: async () =>
      unwrap(await api.GET('/api/v2/assets/{kind}/{id}', { params: { path: { kind, id } } })),
  })

/** At most 50 values: the pickers search as the user types. */
export const choicesQuery = (kind: AssetType, field: ChoiceField, entityId: number, search: string) =>
  queryOptions({
    queryKey: ['choices', kind, field, entityId, search],
    queryFn: async () =>
      unwrap(
        await api.GET('/api/v2/assets/{kind}/choices/{field}', {
          params: {
            path: { kind, field },
            query: { entity_id: entityId, search: search || undefined },
          },
        }),
      ).items,
    staleTime: 30_000,
    placeholderData: keepPreviousData,
  })

export async function createAsset(kind: AssetType, entityId: number, fields: AssetFields): Promise<AssetDetail> {
  return unwrap(
    await api.POST('/api/v2/assets/{kind}', {
      params: { path: { kind } },
      body: { ...fields, entity_id: entityId },
    }),
  )
}

export async function updateAsset(kind: AssetType, id: number, fields: AssetFields): Promise<AssetDetail> {
  return unwrap(
    await api.PATCH('/api/v2/assets/{kind}/{id}', { params: { path: { kind, id } }, body: fields }),
  )
}

export async function setTrashed(kind: AssetType, id: number, trashed: boolean): Promise<AssetDetail> {
  const options = { params: { path: { kind, id } } }
  return unwrap(
    trashed
      ? await api.POST('/api/v2/assets/{kind}/{id}/trash', options)
      : await api.POST('/api/v2/assets/{kind}/{id}/restore', options),
  )
}
