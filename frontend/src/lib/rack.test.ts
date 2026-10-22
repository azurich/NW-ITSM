import { describe, expect, it } from 'vitest'
import { columnLetters, parsePosition } from './rack'

describe('columnLetters', () => {
  it('counts like the classic interface', () => {
    expect([1, 2, 26, 27, 52, 53, 702, 703].map(columnLetters)).toEqual([
      'A',
      'B',
      'Z',
      'AA',
      'AZ',
      'BA',
      'ZZ',
      'AAA',
    ])
  })
})

describe('parsePosition', () => {
  it('reads a place of a room', () => {
    expect(parsePosition('2,3')).toEqual({ column: 'B', row: 3 })
    expect(parsePosition('28, 1')).toEqual({ column: 'AB', row: 1 })
  })

  it('has none for the other values', () => {
    expect(parsePosition('0')).toBeNull()
    expect(parsePosition('')).toBeNull()
    expect(parsePosition(null)).toBeNull()
    expect(parsePosition('0,1')).toBeNull()
    expect(parsePosition('a,1')).toBeNull()
  })
})
