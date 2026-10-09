import { keepPreviousData, queryOptions } from '@tanstack/react-query'
import { api, unwrap, type Schemas } from './client'
import type { paths } from './schema'

export type ComputerPage = Schemas['ComputerPage']
export type ComputerDetail = Schemas['ComputerDetail']

type ListQuery = NonNullable<paths['/api/v2/computers']['get']['parameters']['query']>
export type ComputerSort = NonNullable<ListQuery['sort']>
export type SortOrder = NonNullable<ListQuery['order']>

export const PER_PAGE = 20

export interface ComputerListParams {
  page: number
  search: string
  sort: ComputerSort
  order: SortOrder
}

export const computersQuery = (params: ComputerListParams) =>
  queryOptions({
    queryKey: ['computers', params],
    queryFn: async () =>
      unwrap(
        await api.GET('/api/v2/computers', {
          params: {
            query: {
              page: params.page,
              per_page: PER_PAGE,
              search: params.search || undefined,
              sort: params.sort,
              order: params.order,
            },
          },
        }),
      ),
    placeholderData: keepPreviousData,
  })

export const computerQuery = (id: number) =>
  queryOptions({
    queryKey: ['computer', id],
    queryFn: async () => unwrap(await api.GET('/api/v2/computers/{id}', { params: { path: { id } } })),
  })
