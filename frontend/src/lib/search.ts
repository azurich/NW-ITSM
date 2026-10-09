import type { ComputerListParams, ComputerSort, SortOrder } from '../api/computers'

/** Search parameters of the computer list, as found in the URL. */
export interface ComputerListSearch {
  page?: number
  search?: string
  sort?: ComputerSort
  order?: SortOrder
}

const SORTS: readonly ComputerSort[] = [
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
export function parseComputerListSearch(raw: Record<string, unknown>): ComputerListSearch {
  const page = Number(raw.page)
  return {
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    search: typeof raw.search === 'string' && raw.search.trim() !== '' ? raw.search : undefined,
    sort: SORTS.find((sort) => sort === raw.sort),
    order: raw.order === 'desc' ? 'desc' : undefined,
  }
}

export function withDefaults(search: ComputerListSearch): ComputerListParams {
  return {
    page: search.page ?? 1,
    search: search.search?.trim() ?? '',
    sort: search.sort ?? 'name',
    order: search.order ?? 'asc',
  }
}
