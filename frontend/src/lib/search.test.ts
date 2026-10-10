import { describe, expect, it } from 'vitest'
import { parseAssetListSearch, withDefaults } from './search'

describe('parseAssetListSearch', () => {
  it('keeps valid values', () => {
    expect(
      parseAssetListSearch({ page: '3', search: 'pc', sort: 'serial', order: 'desc', trash: true }),
    ).toEqual({
      page: 3,
      search: 'pc',
      sort: 'serial',
      order: 'desc',
      trash: true,
    })
  })

  it('drops invalid or default values', () => {
    expect(
      parseAssetListSearch({ page: '-2', search: '  ', sort: 'password', order: 'up', trash: 'yes' }),
    ).toEqual({
      page: undefined,
      search: undefined,
      sort: undefined,
      order: undefined,
      trash: undefined,
    })
    expect(parseAssetListSearch({ page: 1.5 }).page).toBeUndefined()
    expect(parseAssetListSearch({ page: 1 }).page).toBeUndefined()
    expect(parseAssetListSearch({ trash: false }).trash).toBeUndefined()
  })
})

describe('withDefaults', () => {
  it('fills the API parameters', () => {
    expect(withDefaults({})).toEqual({ page: 1, search: '', sort: 'name', order: 'asc', trash: false })
    expect(withDefaults({ search: ' pc ' }).search).toBe('pc')
  })
})
