import type { SelectableEntity, SessionInfo } from '../api/session'

/**
 * Entities the session works on (the legacy active entities): items can be
 * created and changed there only, not in the parents whose shared items
 * are visible.
 */
export function activeEntities(
  entities: SessionInfo['entities'],
  selectable: readonly SelectableEntity[],
): SelectableEntity[] {
  const selected = entities.selected
  if (!selected) {
    return [...selectable]
  }
  if (!entities.recursive) {
    return selectable.filter((entity) => entity.id === selected.id)
  }
  const active = new Set([selected.id])
  // Parents come before their children when sorted by level.
  for (const entity of [...selectable].sort((a, b) => a.level - b.level)) {
    if (entity.parent_id != null && active.has(entity.parent_id)) {
      active.add(entity.id)
    }
  }
  return selectable.filter((entity) => active.has(entity.id))
}
