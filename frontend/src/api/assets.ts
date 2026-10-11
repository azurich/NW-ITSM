import { keepPreviousData, queryOptions } from '@tanstack/react-query'
import { api, toApiError, unwrap, type Schemas } from './client'
import type { paths } from './schema'

export type AssetType = Schemas['AssetType']
export type AssetPage = Schemas['AssetPage']
export type AssetSummary = Schemas['AssetSummary']
export type AssetDetail = Schemas['AssetDetail']
export type AssetFields = Schemas['AssetFields']
export type ChoiceField = Schemas['ChoiceField']
export type Choice = Schemas['Choice']
export type DuplicateNotice = Schemas['DuplicateNotice']
export type RoomPosition = Schemas['RoomPosition']
export type AssetDefaults = Schemas['AssetDefaults']
export type TemplateList = Schemas['TemplateList']
export type TemplateSummary = Schemas['TemplateSummary']
/** Fields of a template: those of its items, and its own name. */
export type TemplateFields = Schemas['TemplateFields']
/** Connector or feature of a type (`has_*`). */
export type Feature = keyof Schemas['Features']

type ListQuery = NonNullable<paths['/api/v2/assets/{kind}']['get']['parameters']['query']>
export type AssetSort = NonNullable<ListQuery['sort']>
export type SortOrder = NonNullable<ListQuery['order']>

/** Common fields some types do not have. */
export type OptionalField =
  | 'type'
  | 'manufacturer'
  | 'model'
  | 'serial'
  | 'inventory_number'
  | 'location'
  | 'user'
  | 'group'
  | 'contact'
  | 'contact_number'
  | 'state'

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
  /** Common fields the type does not have. */
  without?: readonly OptionalField[]
  /** No templates (clusters). */
  noTemplates?: boolean
  /** Server room, place, door, units, size and color of the racks. */
  rack?: boolean
  /** Number of power supplies (enclosures). */
  powerSupplies?: boolean
  /** Version (clusters). */
  version?: boolean
  /**
   * Software: a category, a publisher (the manufacturer), associable to
   * tickets, an upgrade of another software, valid licenses.
   */
  software?: boolean
  /** Software can be installed on it. */
  installations?: boolean
  /**
   * Licenses of a software: their software, parent license, versions,
   * number, over-quota, expiration and validity.
   */
  license?: boolean
}

/** The datacenter items have no user, group nor contact. */
const NO_USER = ['user', 'group', 'contact', 'contact_number'] as const

export const ASSET_TYPES: Record<AssetType, AssetTypeInfo> = {
  computers: {
    right: 'computer',
    legacy: 'computer',
    uuid: true,
    network: true,
    updateSource: true,
    installations: true,
  },
  monitors: {
    right: 'monitor',
    legacy: 'monitor',
    installations: true,
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
    installations: true,
    network: true,
    global: true,
    memory: 'text',
    pageCounters: true,
    features: ['has_serial', 'has_parallel', 'has_usb', 'has_ethernet', 'has_wifi'],
  },
  'network-equipments': {
    right: 'networking',
    legacy: 'networkequipment',
    network: true,
    memory: 'mib',
    installations: true,
  },
  peripherals: { right: 'peripheral', legacy: 'peripheral', brand: true, global: true, installations: true },
  phones: {
    right: 'phone',
    legacy: 'phone',
    installations: true,
    brand: true,
    numberLine: true,
    global: true,
    powerSupply: true,
    features: ['has_headset', 'has_speaker'],
  },
  racks: { right: 'datacenter', legacy: 'rack', without: NO_USER, rack: true },
  enclosures: { right: 'datacenter', legacy: 'enclosure', without: [...NO_USER, 'type'], powerSupplies: true },
  pdus: { right: 'datacenter', legacy: 'pdu', without: NO_USER },
  'passive-dc-equipments': { right: 'datacenter', legacy: 'passivedcequipment', without: NO_USER },
  clusters: {
    right: 'cluster',
    legacy: 'cluster',
    uuid: true,
    updateSource: true,
    version: true,
    noTemplates: true,
    without: [...NO_USER, 'manufacturer', 'model', 'serial', 'inventory_number', 'location'],
  },
  softwares: {
    right: 'software',
    legacy: 'software',
    software: true,
    without: ['type', 'model', 'serial', 'inventory_number', 'contact', 'contact_number', 'state'],
  },
  // Their templates stay in the classic interface.
  'software-licenses': {
    right: 'license',
    legacy: 'softwarelicense',
    license: true,
    noTemplates: true,
    without: ['model', 'contact', 'contact_number'],
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

/** Filters of the choices of a license: its software, itself. */
export interface ChoiceFilters {
  software_id?: number
  exclude_id?: number
}

/** At most 50 values: the pickers search as the user types. */
export const choicesQuery = (
  kind: AssetType,
  field: ChoiceField,
  entityId: number,
  search: string,
  filters: ChoiceFilters = {},
) =>
  queryOptions({
    queryKey: ['choices', kind, field, entityId, search, filters],
    queryFn: async () =>
      unwrap(
        await api.GET('/api/v2/assets/{kind}/choices/{field}', {
          params: {
            path: { kind, field },
            query: { entity_id: entityId, search: search || undefined, ...filters },
          },
        }),
      ).items,
    staleTime: 30_000,
    placeholderData: keepPreviousData,
  })

/**
 * Places of a server room no other rack takes, like the classic form offers
 * them; `current`, the place of the rack being edited, is offered too.
 */
export const roomPositionsQuery = (roomId: number, current?: string) =>
  queryOptions({
    queryKey: ['room-positions', roomId, current],
    queryFn: async () =>
      unwrap(
        await api.GET('/api/v2/assets/racks/positions', {
          params: { query: { room_id: roomId, current: current || undefined } },
        }),
      ).items,
  })

/** Values of a new item set by the settings (`getEmpty`). */
export const defaultsQuery = (kind: AssetType) =>
  queryOptions({
    queryKey: ['asset-defaults', kind],
    queryFn: async () =>
      unwrap(await api.GET('/api/v2/assets/{kind}/defaults', { params: { path: { kind } } })),
    staleTime: 60_000,
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

/** Deletes an asset of the trash for good. */
export async function purgeAsset(kind: AssetType, id: number): Promise<void> {
  const { response, error } = await api.DELETE('/api/v2/assets/{kind}/{id}', { params: { path: { kind, id } } })
  if (!response.ok) {
    throw toApiError(response.status, error)
  }
}

/** Templates of a type, by name. */
export const templatesQuery = (kind: AssetType) =>
  queryOptions({
    queryKey: ['templates', kind],
    queryFn: async () =>
      unwrap(await api.GET('/api/v2/assets/{kind}/templates', { params: { path: { kind } } })),
  })

export const templateQuery = (kind: AssetType, id: number) =>
  queryOptions({
    queryKey: ['template', kind, id],
    queryFn: async () =>
      unwrap(await api.GET('/api/v2/assets/{kind}/templates/{id}', { params: { path: { kind, id } } })),
  })

export async function createTemplate(kind: AssetType, entityId: number, fields: TemplateFields): Promise<AssetDetail> {
  return unwrap(
    await api.POST('/api/v2/assets/{kind}/templates', {
      params: { path: { kind } },
      body: { ...fields, entity_id: entityId },
    }),
  )
}

export async function updateTemplate(kind: AssetType, id: number, fields: TemplateFields): Promise<AssetDetail> {
  return unwrap(
    await api.PATCH('/api/v2/assets/{kind}/templates/{id}', { params: { path: { kind, id } }, body: fields }),
  )
}

/** Deletes a template for good. */
export async function purgeTemplate(kind: AssetType, id: number): Promise<void> {
  const { response, error } = await api.DELETE('/api/v2/assets/{kind}/templates/{id}', {
    params: { path: { kind, id } },
  })
  if (!response.ok) {
    throw toApiError(response.status, error)
  }
}

/** Creates an item from a template: the fields not given take its values. */
export async function createFromTemplate(kind: AssetType, id: number, fields: AssetFields): Promise<AssetDetail> {
  return unwrap(
    await api.POST('/api/v2/assets/{kind}/templates/{id}/items', {
      params: { path: { kind, id } },
      body: fields,
    }),
  )
}

export type SoftwareVersion = Schemas['Version']
export type VersionFields = Schemas['VersionFields']

/** Versions of a software, by name. */
export const versionsQuery = (softwareId: number) =>
  queryOptions({
    queryKey: ['versions', softwareId],
    queryFn: async () =>
      unwrap(await api.GET('/api/v2/assets/softwares/{id}/versions', { params: { path: { id: softwareId } } })),
  })

export async function createVersion(softwareId: number, fields: VersionFields): Promise<SoftwareVersion> {
  return unwrap(
    await api.POST('/api/v2/assets/softwares/{id}/versions', {
      params: { path: { id: softwareId } },
      body: fields,
    }),
  )
}

export async function updateVersion(softwareId: number, id: number, fields: VersionFields): Promise<SoftwareVersion> {
  return unwrap(
    await api.PATCH('/api/v2/assets/softwares/{id}/versions/{version_id}', {
      params: { path: { id: softwareId, version_id: id } },
      body: fields,
    }),
  )
}

/** Deletes a version for good, with its installations. */
export async function purgeVersion(softwareId: number, id: number): Promise<void> {
  const { response, error } = await api.DELETE('/api/v2/assets/softwares/{id}/versions/{version_id}', {
    params: { path: { id: softwareId, version_id: id } },
  })
  if (!response.ok) {
    throw toApiError(response.status, error)
  }
}

export type Installation = Schemas['Installation']

/** Software installed on an asset, by software and version. */
export const installationsQuery = (kind: AssetType, id: number) =>
  queryOptions({
    queryKey: ['installations', kind, id],
    queryFn: async () =>
      unwrap(await api.GET('/api/v2/assets/{kind}/{id}/software', { params: { path: { kind, id } } })),
  })

export async function installSoftware(kind: AssetType, id: number, versionId: number): Promise<Installation> {
  return unwrap(
    await api.POST('/api/v2/assets/{kind}/{id}/software', {
      params: { path: { kind, id } },
      body: { version_id: versionId },
    }),
  )
}

/** Uninstalls a software, for good. */
export async function uninstallSoftware(kind: AssetType, id: number, installationId: number): Promise<void> {
  const { response, error } = await api.DELETE('/api/v2/assets/{kind}/{id}/software/{installation_id}', {
    params: { path: { kind, id, installation_id: installationId } },
  })
  if (!response.ok) {
    throw toApiError(response.status, error)
  }
}

/** Licenses of a software, by name. */
export const softwareLicensesQuery = (softwareId: number, trash = false) =>
  queryOptions({
    queryKey: ['assets', 'software-licenses', { software: softwareId, trash }],
    queryFn: async () =>
      unwrap(
        await api.GET('/api/v2/assets/{kind}', {
          params: {
            path: { kind: 'software-licenses' },
            query: { software_id: softwareId, per_page: 100, sort: 'name', trash: trash || undefined },
          },
        }),
      ),
  })
