/** Column of a server room in letters, like the classic interface: A to Z, then AA. */
export function columnLetters(column: number): string {
  let letters = ''
  for (let rest = column; rest > 0; rest = Math.floor((rest - 1) / 26)) {
    letters = String.fromCharCode(65 + ((rest - 1) % 26)) + letters
  }
  return letters
}

/** Column (in letters) and row of a place of a room, `"<column>,<row>"`; null for none. */
export function parsePosition(position: string | null | undefined): { column: string; row: number } | null {
  const match = /^(\d+),\s?(\d+)$/.exec(position ?? '')
  if (!match) {
    return null
  }
  const column = Number(match[1])
  const row = Number(match[2])
  return column > 0 && row > 0 ? { column: columnLetters(column), row } : null
}

/** Door orientations of a rack, by stored value. */
export const ORIENTATIONS = { 1: 'north', 2: 'east', 3: 'south', 4: 'west' } as const
