import { describe, it, expect } from "vitest";
import { MegaRng, backgroundRng } from "./megaRng";

/**
 * The port is pinned against a fight the REAL GAME drew, not against itself.
 *
 * Live run VISCMP1 (`logs/20260828_022503_Ironclad_VISCMP1.jsonl`): the floor-6
 * Seapunk fight reports `rng.run_seed` 44815703 and stands on map node
 * (col 5, row 5). The game drew `underdocks_01_c` there — the bucket-01 variant
 * carrying the lit barred window, which is exactly the difference that made the
 * workbench's room read as "not quite the game's".
 */
const UNDERDOCKS_BUCKETS: Record<string, string[]> = {
  "00": ["00_a"],
  "01": ["01_a", "01_b", "01_c"],
  "02": ["02_a", "02_b", "02_c"],
  "03": ["03_b", "03_c"],
  "04": ["04_a", "04_b"],
};

function rollRoom(rng: MegaRng): Record<string, string> {
  const picks: Record<string, string> = {};
  for (const key of Object.keys(UNDERDOCKS_BUCKETS).sort()) {
    picks[key] = rng.nextItem(UNDERDOCKS_BUCKETS[key])!;
  }
  picks.fg = rng.nextItem(["a", "b"])!;
  return picks;
}

describe("MegaRng", () => {
  it("reproduces the variant the live game drew", () => {
    expect(rollRoom(backgroundRng(44815703, 5, 5))["01"]).toBe("01_c");
  });

  it("gives each map node its own room", () => {
    // `row + col * 747` — the whole point of the offset is that neighbouring
    // nodes of one run do not draw the same background.
    expect(rollRoom(backgroundRng(44815703, 5, 5))).not.toEqual(
      rollRoom(backgroundRng(44815703, 6, 5)),
    );
  });

  it("is a pure function of the node", () => {
    expect(rollRoom(backgroundRng(44815703, 5, 5))).toEqual(
      rollRoom(backgroundRng(44815703, 5, 5)),
    );
  });

  it("draws in [0, 1) and truncates rather than rounding", () => {
    const rng = new MegaRng(1);
    for (let i = 0; i < 500; i++) {
      const d = rng.nextDouble();
      expect(d).toBeGreaterThanOrEqual(0);
      expect(d).toBeLessThan(1);
    }
    // NextInt(n) must never reach n — `(int)(NextDouble() * n)` truncates, and
    // an off-by-one here indexes past the end of a variant list.
    const r2 = new MegaRng(7);
    for (let i = 0; i < 500; i++) {
      const v = r2.nextInt(3);
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThan(3);
    }
  });

  it("returns undefined for an empty list, like default(T)", () => {
    expect(new MegaRng(1).nextItem([])).toBeUndefined();
  });

  it("wraps the uint seed instead of overflowing", () => {
    // A run seed near 2^32 plus the node offset must stay a uint.
    expect(() => rollRoom(backgroundRng(4294967290, 9, 9))).not.toThrow();
  });
});
