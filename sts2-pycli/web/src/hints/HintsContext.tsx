import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

/**
 * Global "show me what the AI thinks *without my asking*" switch — **off by
 * default**.
 *
 * The workbench grew a lot of advice surfaces (the PPO policy panel, MCTS
 * combat hints, the EV analyzers on card rewards / shops / events, synergy and
 * archetype chips, the map's planned route). Every one of them used to render
 * on its own condition, so a human sitting down to play was told the answer
 * before deciding — and autobattle mode, whose whole point is to withhold the
 * champion's pick until after you commit, leaked it through the policy panel
 * sitting right under the options.
 *
 * **What this flag governs is UNPROMPTED advice, not access to the tools.**
 * The line is whether a surface can tell you something you did not ask for:
 *
 * - GATED (renders an opinion on its own): the PPO policy panel, the auto
 *   combat hint and its per-card win-prob overlays, the card-reward / shop /
 *   event EV analyzers, the synergy + archetype chips, and the map's green
 *   planned-route overlay — a route drawn across the map is the AI answering
 *   the map decision for you.
 * - UNGATED (press-first — nothing appears until you click): the hint button
 *   and H, play-best (G), autoplay (A) and play-to-end (F), the search-tree
 *   inspector (S), the regret coach toggle, the regret review and run review
 *   docks, and the deck analyzer. Reaching for one of these IS the deliberate
 *   act the switch exists to require, so making you flip a second switch first
 *   only costs a click.
 *
 * Off = play the game as a human: cards, prices, HP, deck, map, and nothing
 * that evaluates your options until you ask it to.
 *
 * Scope: **live play only.** Replay is for reviewing a recorded AI run, so its
 * surfaces stay visible — there is no drill to spoil there.
 */

const STORAGE_KEY = "hintsOn";

interface HintsValue {
  /** True when AI advice may be displayed. */
  on: boolean;
  setOn: (v: boolean) => void;
  toggle: () => void;
}

// The default is deliberately OFF, so a component rendered without a provider
// (a unit test, a stray subtree) hides advice rather than leaking it.
const Ctx = createContext<HintsValue>({ on: false, setOn: () => {}, toggle: () => {} });

export function useHints(): HintsValue {
  return useContext(Ctx);
}

function readStored(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false; // private mode / storage disabled — default to hidden
  }
}

export function HintsProvider({ children }: { children: ReactNode }) {
  const [on, setOn] = useState<boolean>(readStored);
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, on ? "1" : "0");
    } catch {
      /* storage unavailable — the flag still works for this session */
    }
  }, [on]);
  const value = useMemo(() => ({ on, setOn, toggle: () => setOn((v) => !v) }), [on]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/** Test/story helper: force a value without touching storage. */
export function HintsFixture({ on, children }: { on: boolean; children: ReactNode }) {
  const value = useMemo(() => ({ on, setOn: () => {}, toggle: () => {} }), [on]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
