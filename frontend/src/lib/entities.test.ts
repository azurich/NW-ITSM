import { describe, expect, it } from 'vitest'
import type { SelectableEntity } from '../api/session'
import { activeEntities } from './entities'

const entity = (id: number, parent_id: number | null, level: number): SelectableEntity => ({
  id,
  parent_id,
  level,
  name: `E${id}`,
  completename: `E${id}`,
  recursive_allowed: true,
})

// 0 ─┬─ 1 ── 3 ── 4
//    └─ 2
const tree = [entity(4, 3, 4), entity(0, null, 1), entity(1, 0, 2), entity(2, 0, 2), entity(3, 1, 3)]
const ids = (entities: SelectableEntity[]) => entities.map((item) => item.id).sort()

describe('activeEntities', () => {
  it('is every entity when none is selected', () => {
    expect(ids(activeEntities({ selected: null, recursive: true, count: 5, unrestricted: true }, tree))).toEqual([
      0, 1, 2, 3, 4,
    ])
  })

  it('is the selected entity and its sub-entities', () => {
    const selected = { id: 1, name: 'E1' }
    expect(ids(activeEntities({ selected, recursive: true, count: 3, unrestricted: false }, tree))).toEqual([
      1, 3, 4,
    ])
    expect(ids(activeEntities({ selected, recursive: false, count: 1, unrestricted: false }, tree))).toEqual([1])
  })
})
