import { describe, expect, it } from 'vitest'
import { parseComputerListSearch, withDefaults } from './search'

describe('parseComputerListSearch', () => {
  it('keeps valid values', () => {
    expect(parseComputerListSearch({ page: '3', search: 'pc', sort: 'serial', order: 'desc' })).toEqual({
      page: 3,
      search: 'pc',
      sort: 'serial',
      order: 'desc',
    })
  })

  it('drops invalid or default values', () => {
    expect(parseComputerListSearch({ page: '-2', search: '  ', sort: 'password', order: 'up' })).toEqual({
      page: undefined,
      search: undefined,
      sort: undefined,
      order: undefined,
    })
    expect(parseComputerListSearch({ page: 1.5 }).page).toBeUndefined()
    expect(parseComputerListSearch({ page: 1 }).page).toBeUndefined()
  })
})

describe('withDefaults', () => {
  it('fills the API parameters', () => {
    expect(withDefaults({})).toEqual({ page: 1, search: '', sort: 'name', order: 'asc' })
    expect(withDefaults({ search: ' pc ' }).search).toBe('pc')
  })
})
