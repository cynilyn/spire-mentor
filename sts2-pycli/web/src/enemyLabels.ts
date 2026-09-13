import type { Enemy } from "./api";

/**
 * Map an enemy slot index to something a human recognises.
 *
 * The reasoning panel and the search-tree view both used to render a target as
 * its raw index ("Strike → 1"), which is only meaningful if you already know
 * the encounter's slot order. Names are NOT unique (two Cultists, three
 * Louses), so a duplicated name is disambiguated by its 1-based position among
 * the enemies sharing it — "Louse #2" — rather than by the slot index, which
 * is the thing that was unreadable in the first place.
 *
 * Falls back to "enemy N" when the caller has no enemy list, e.g. a search
 * tree opened from a step recorded without one.
 */
export function enemyLabeller(enemies: Enemy[] | undefined): (index: number) => string {
  const byIndex = new Map<number, string>();
  if (enemies && enemies.length > 0) {
    const total = new Map<string, number>();
    for (const e of enemies) total.set(e.name, (total.get(e.name) ?? 0) + 1);
    const seen = new Map<string, number>();
    for (const e of enemies) {
      const nth = (seen.get(e.name) ?? 0) + 1;
      seen.set(e.name, nth);
      byIndex.set(e.index, (total.get(e.name) ?? 0) > 1 ? `${e.name} #${nth}` : e.name);
    }
  }
  return (index: number) => byIndex.get(index) ?? `enemy ${index}`;
}
