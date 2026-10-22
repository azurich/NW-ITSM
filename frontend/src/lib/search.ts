import type { AssetListParams, AssetSort, SortOrder } from '../api/assets'

/** Search parameters of an asset list, as found in the URL. */
export interface AssetListSearch {
  page?: number
  search?: string
  sort?: AssetSort
  order?: SortOrder
  /** Show the trash instead of the other items. */
  trash?: true
}

const SORTS: readonly AssetSort[] = [
  'name',
  'serial',
  'inventory_number',
  'entity',
  'location',
  'type',
  'manufacturer',
  'state',
  'updated_at',
]

/** Keeps only valid values: a hand-edited URL never breaks the page. */
export function parseAssetListSearch(raw: Record<string, unknown>): AssetListSearch {
  const page = Number(raw.page)
  return {
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    search: typeof raw.search === 'string' && raw.search.trim() !== '' ? raw.search : undefined,
    sort: SORTS.find((sort) => sort === raw.sort),
    order: raw.order === 'desc' ? 'desc' : undefined,
    trash: raw.trash === true || raw.trash === 'true' ? true : undefined,
  }
}

export function withDefaults(search: AssetListSearch): AssetListParams {
  return {
    page: search.page ?? 1,
    search: search.search?.trim() ?? '',
    sort: search.sort ?? 'name',
    order: search.order ?? 'asc',
    trash: search.trash ?? false,
  }
}

/** Search parameters of the creation page. */
export interface AssetCreateSearch {
  /** Template to start from; `0` for none. */
  template?: number
  /** A license of this software (from its page), and its name. */
  software?: number
  software_name?: string
}

export function parseAssetCreateSearch(raw: Record<string, unknown>): AssetCreateSearch {
  const template = Number(raw.template)
  const software = Number(raw.software)
  const search: AssetCreateSearch = {
    template: raw.template !== undefined && Number.isInteger(template) && template >= 0 ? template : undefined,
  }
  if (raw.software !== undefined && Number.isInteger(software) && software > 0) {
    search.software = software
    search.software_name = typeof raw.software_name === 'string' ? raw.software_name : undefined
  }
  return search
}
