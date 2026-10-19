import { type MouseEvent as ReactMouseEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import type { Enemy, MctsConfig, SearchTree as SearchTreeData, SearchTreeNode } from "../api";
import NumberInput from "./NumberInput";
import { enemyLabeller } from "../enemyLabels";

/**
 * Agent search-tree inspector: a modal node-link view of the MCTS search for
 * one combat decision. Nodes are sized by visit count and colored by Q (the
 * player-perspective expected value); edge width tracks child visits; the
 * most-visited path (the engine's principal variation) is highlighted gold.
 *
 * The tree has hundreds of nodes, so it renders lazily: the root is expanded,
 * each node shows its top-K children by visits, and clicking a node expands it.
 * A min-visits filter and the sims selector (re-runs the search) tame density.
 * Selecting a node shows its full stats — N, Q, win/tie/loss, the UCT
 * exploration term U, and MCTS-solver proven-win/loss flags. The engine uses a
 * uniform prior, so there is no learned policy P to show.
 *
 * The tree alternates DECISION nodes (the player picks an action — circles ●) and CHANCE
 * nodes (nature resolves a stochastic transition like a card draw or intent
 * roll — diamonds ◆). A chance node's children ARE its sampled outcomes (each
 * outcome's probability p = child.visits / chance.visits), its value is the
 * expectimax E[V] = Σ p·V (labelled "E", not "Q"), and DPW caps how many
 * outcomes are sampled (k/cap, possibly truncated). The "collapse dice" toggle
 * folds the chance layer so the tree reads like the legacy PIMC move line: a
 * decision connects straight to the modal (most-visited) outcome through its
 * action, honestly labelled with the chance node's E[V]. Older / PIMC trees
 * have no chance nodes (node_kind defaults to "decision").
 */
const COL_GAP = 152;
const ROW_GAP = 58;
const PAD = 34;
const MAX_R = 16;
const SIM_CHOICES = [300, 1000, 3000, 10000, 50000, 150000];

function qColor(q: number): string {
  // q ∈ [-1,1] → red (loss) ⟶ amber ⟶ green (win).
  const t = Math.max(0, Math.min(1, (q + 1) / 2));
  return `hsl(${Math.round(t * 122)} 62% 44%)`;
}

export default function SearchTree({
  tree,
  busy,
  config,
  onRun,
  onClose,
  potionNames,
  enemies,
}: {
  tree: SearchTreeData | null;
  busy?: boolean;
  config: MctsConfig;
  onRun: (config: MctsConfig) => void;
  onClose: () => void;
  // Potion display names by slot, so "Potion s0" edges show the actual potion.
  potionNames?: (string | null | undefined)[];
  // Combatants of the searched position, so a target edge reads "→ Cultist"
  // instead of "→ enemy 0". Same reason as potionNames.
  enemies?: Enemy[];
}) {
  const enemyName = enemyLabeller(enemies);
  // Draft knobs: edited locally, applied (search re-runs) only on "run". Keeps
  // an expensive 150k-sim search from firing on every keystroke.
  const [draft, setDraft] = useState<MctsConfig>(config);
  useEffect(() => setDraft(config), [config]);
  const dirty =
    draft.sims !== config.sims || draft.c_puct !== config.c_puct ||
    draft.depth !== config.depth || draft.rollout_policy !== config.rollout_policy ||
    draft.seed !== config.seed ||
    (draft.dominance_pruning ?? false) !== (config.dominance_pruning ?? false);
  const setK = <K extends keyof MctsConfig>(k: K, v: MctsConfig[K]) =>
    setDraft((d) => ({ ...d, [k]: v }));
  const run = () => onRun(draft);
  const [expanded, setExpanded] = useState<Set<number>>(new Set([0]));
  const [selected, setSelected] = useState<number>(0);
  const [topK, setTopK] = useState(5);
  const [minVisits, setMinVisits] = useState(1);
  const [view, setView] = useState<"outline" | "graph">("outline");
  // Fold the chance layer so a decision connects straight to the modal outcome
  // through its action (PIMC-shaped). Off = full depth (the user's default).
  const [collapseChance, setCollapseChance] = useState(false);
  const [hover, setHover] = useState<{ id: number; x: number; y: number } | null>(null);
  // Zoom: "fit" auto-scales to the viewport (but never below a readable floor,
  // scrolling instead); a number is an explicit user zoom. Default fit.
  const [zoom, setZoom] = useState<number | "fit">("fit");
  // Measure the viewport so "fit" can size the graph.
  const wrapRef = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    measure();
    // Prefer ResizeObserver; fall back to window resize (e.g. jsdom in tests).
    if (typeof ResizeObserver !== "undefined") {
      const ro = new ResizeObserver(measure);
      ro.observe(el);
      return () => ro.disconnect();
    }
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") { e.preventDefault(); onClose(); }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const byId = useMemo(
    () => new Map((tree?.nodes ?? []).map((n) => [n.id, n])),
    [tree],
  );
  const rootId = tree?.root ?? 0;

  // A node's own type. PIMC / older trees omit node_kind → treat as "decision".
  const nodeKind = (n: SearchTreeNode): "decision" | "chance" => n.node_kind ?? "decision";
  const isChance = (n: SearchTreeNode | undefined): boolean =>
    !!n && nodeKind(n) === "chance";
  // The most-visited child of a node (a chance node's modal outcome).
  function modalChild(n: SearchTreeNode): SearchTreeNode | undefined {
    let best: SearchTreeNode | undefined;
    for (const cid of n.children) {
      const c = byId.get(cid);
      if (c && (!best || c.visits > best.visits)) best = c;
    }
    return best;
  }

  // ── Display tree ────────────────────────────────────────────────────────
  // One central edge resolver routes ALL traversal (outline, graph layout, PV,
  // expand-all / best-line). A display edge keeps two ids apart: `displayId`
  // (the node whose stats/label we render for this row) and `childId` (whose
  // children we recurse into). They differ only for a FOLDED chance edge.
  type DispEdge = {
    childId: number;          // recurse into this node's displayChildren
    displayId: number;        // render this node's stats / label for the edge
    viaChance?: number;       // chance node id folded into this edge (collapse on)
    outcome?: boolean;        // this edge is a chance OUTCOME (probability edge)
  };
  // The recursion id for a visible displayId: a folded chance node recurses
  // through its modal outcome; everything else recurses through itself.
  function recurseIdFor(displayId: number): number {
    const n = byId.get(displayId);
    if (collapseChance && n && isChance(n)) return modalChild(n)?.id ?? displayId;
    return displayId;
  }
  function displayChildren(id: number): DispEdge[] {
    const node = byId.get(id);
    if (!node) return [];
    const edges: DispEdge[] = [];
    for (const cid of node.children) {
      const c = byId.get(cid);
      if (!c) continue;
      if (collapseChance && isChance(c)) {
        // Fold the dice: show the chance node (its action + E[V]) but recurse
        // into its modal outcome's children → one child per action, PIMC-shaped.
        const modal = modalChild(c);
        edges.push({ childId: modal?.id ?? c.id, displayId: c.id, viaChance: c.id });
      } else {
        edges.push({ childId: cid, displayId: cid, outcome: c.kind === "outcome" });
      }
    }
    return edges;
  }

  // Principal variation = most-visited display-edge chain from the root.
  const pvSet = useMemo(() => {
    const s = new Set<number>();
    let recurseId: number = rootId;
    let displayId: number = rootId;
    while (byId.has(displayId) && !s.has(displayId)) {
      s.add(displayId);
      const edges = displayChildren(recurseId);
      if (edges.length === 0) break;
      let best = edges[0];
      for (const e of edges) {
        if ((byId.get(e.displayId)?.visits ?? 0) > (byId.get(best.displayId)?.visits ?? 0)) best = e;
      }
      recurseId = best.childId;
      displayId = best.displayId;
    }
    return s;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [byId, rootId, collapseChance]);

  // Display edges we actually render for an expanded node: the displayId node's
  // visits ≥ filter, sorted by visits desc, capped at topK. + hidden count.
  const shownDisplay = (recurseId: number): { edges: DispEdge[]; hidden: number } => {
    const kids = displayChildren(recurseId)
      .map((e) => ({ e, vn: byId.get(e.displayId) }))
      .filter((x): x is { e: DispEdge; vn: SearchTreeNode } => !!x.vn && x.vn.visits >= minVisits)
      .sort((a, b) => b.vn.visits - a.vn.visits);
    return { edges: kids.slice(0, topK).map((x) => x.e), hidden: Math.max(0, kids.length - topK) };
  };

  // Probability of an outcome node = its visits / its chance-parent's visits.
  function outcomeProb(n: SearchTreeNode): number | null {
    const parent = n.parent != null ? byId.get(n.parent) : undefined;
    if (!parent || parent.visits <= 0) return null;
    return n.visits / parent.visits;
  }
  // DPW sampling badge for a chance node: k sampled of cap, truncated/closed.
  function dpwInfo(n: SearchTreeNode) {
    const k = n.children.length;
    const cap = n.outcome_cap;
    const truncated = cap != null && k >= cap;
    return { k, cap, truncated, closed: !!n.chance_closed };
  }
  // Expectimax outcome spread = [min, max] q over a chance node's outcomes.
  function outcomeSpread(n: SearchTreeNode): { min: number; max: number } | null {
    const qs = n.children
      .map((c) => byId.get(c)?.q)
      .filter((q): q is number => q != null);
    if (qs.length === 0) return null;
    return { min: Math.min(...qs), max: Math.max(...qs) };
  }
  // value label "E" for chance (expectimax), "Q" for a decision node.
  const valueKey = (n: SearchTreeNode) => (isChance(n) ? "E" : "Q");

  // Lay out the visible subset (tidy tree: leaves take columns, parents center).
  // Keyed by displayId; recurses via each frame's recurseId (see displayChildren).
  const built = useMemo(() => {
    if (!tree || !byId.has(rootId)) return null;
    const childMap = new Map<number, number[]>();   // displayId → child displayIds
    const hiddenMap = new Map<number, number>();
    const edgeMap = new Map<number, DispEdge>();      // displayId → its incoming edge
    const recurseMap = new Map<number, number>([[rootId, rootId]]);
    const depth = new Map<number, number>([[rootId, 0]]);
    const order: number[] = [];
    const stack: { displayId: number; recurseId: number; phase: 0 | 1 }[] =
      [{ displayId: rootId, recurseId: rootId, phase: 0 }];
    const col = new Map<number, number>();
    let nextLeaf = 0;
    let maxDepth = 0;
    while (stack.length) {
      const f = stack[stack.length - 1];
      maxDepth = Math.max(maxDepth, depth.get(f.displayId) ?? 0);
      if (f.phase === 0) {
        f.phase = 1;
        if (expanded.has(f.displayId)) {
          const { edges, hidden } = shownDisplay(f.recurseId);
          childMap.set(f.displayId, edges.map((e) => e.displayId));
          hiddenMap.set(f.displayId, hidden);
          for (let i = edges.length - 1; i >= 0; i--) {
            const e = edges[i];
            depth.set(e.displayId, (depth.get(f.displayId) ?? 0) + 1);
            edgeMap.set(e.displayId, e);
            recurseMap.set(e.displayId, e.childId);
            stack.push({ displayId: e.displayId, recurseId: e.childId, phase: 0 });
          }
        } else {
          childMap.set(f.displayId, []);
        }
        continue;
      }
      stack.pop();
      order.push(f.displayId);
      const kids = childMap.get(f.displayId) ?? [];
      if (kids.length === 0) col.set(f.displayId, nextLeaf++);
      else {
        const xs = kids.map((k) => col.get(k)!);
        col.set(f.displayId, (Math.min(...xs) + Math.max(...xs)) / 2);
      }
    }
    const pos = new Map<number, { x: number; y: number }>();
    for (const id of order) {
      pos.set(id, { x: PAD + (col.get(id) ?? 0) * COL_GAP, y: PAD + (depth.get(id) ?? 0) * ROW_GAP });
    }
    const maxCol = Math.max(0, ...[...col.values()]);
    return {
      pos, childMap, hiddenMap, edgeMap, recurseMap,
      width: PAD + maxCol * COL_GAP + COL_GAP,
      height: PAD * 2 + maxDepth * ROW_GAP,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tree, byId, rootId, expanded, topK, minVisits, collapseChance]);

  const rootVisits = byId.get(rootId)?.visits ?? 1;
  const radius = (n: SearchTreeNode) =>
    Math.max(4, Math.min(MAX_R, 4 + 13 * Math.sqrt(n.visits / Math.max(1, rootVisits))));

  // Effective render scale. "fit" auto-scales to the viewport but never below a
  // readable floor (we scroll/pan past that); a number is the user's choice.
  const READABLE_MIN = 0.55;
  const fitRaw = built && box.w && box.h
    ? Math.min(1, (box.w - 12) / built.width, (box.h - 12) / built.height)
    : 1;
  const scale = zoom === "fit" ? Math.max(READABLE_MIN, fitRaw) : zoom;
  const zoomBy = (f: number) =>
    setZoom((z) => Math.round(Math.max(0.3, Math.min(2, (z === "fit" ? scale : z) * f)) * 100) / 100);

  function toggle(displayId: number) {
    setSelected(displayId);
    if (displayChildren(recurseIdFor(displayId)).length === 0) return;
    setExpanded((cur) => {
      const next = new Set(cur);
      if (next.has(displayId)) next.delete(displayId);
      else next.add(displayId);
      return next;
    });
  }
  // Expand every node that can have display children. Modal outcomes that get
  // folded away aren't display ids, so expanding their raw ids is just a no-op.
  const expandAll = () =>
    setExpanded(new Set((tree?.nodes ?? []).filter((nd) => nd.children.length > 0).map((nd) => nd.id)));
  const collapseAll = () => setExpanded(new Set([rootId]));

  // Unfold ONLY the best line (most-visited display edge at each step) from
  // `fromId` down `levels` deeper, without expanding any sibling branches.
  function expandBestLine(fromId: number, levels = 10) {
    setExpanded((cur) => {
      const next = new Set(cur);
      let displayId = fromId;
      for (let i = 0; i < levels; i++) {
        const edges = displayChildren(recurseIdFor(displayId));
        if (edges.length === 0) break;
        next.add(displayId); // reveal this node's children (incl. its best child)
        let best = edges[0];
        for (const e of edges) {
          if ((byId.get(e.displayId)?.visits ?? 0) > (byId.get(best.displayId)?.visits ?? 0)) best = e;
        }
        displayId = best.displayId;
      }
      return next;
    });
  }

  // Display label for a node: resolve potion slots to the actual potion name,
  // and surface a chance node by the stochastic action it resolves.
  function labelOf(nd: SearchTreeNode): string {
    if (nd.id === rootId) return "root";
    if (isChance(nd)) return nd.chance_action ?? nd.label ?? "roll";
    const base = nd.label ?? "?";
    if (nd.kind === "potion" || nd.kind === "drink_potion") {
      const m = (base.match(/s(\d+)/) ?? [])[1];
      const name = m != null ? potionNames?.[Number(m)] : undefined;
      if (name) return name;
    }
    return base;
  }

  // Flattened visible rows for the outline view (DFS over expanded display
  // edges). Each node row carries its incoming display edge (outcome / folded).
  type OutRow =
    | { kind: "node"; id: number; edge?: DispEdge; depth: number }
    | { kind: "more"; key: string; depth: number; count: number };
  function outlineRows(): OutRow[] {
    const rows: OutRow[] = [];
    const walk = (recurseId: number, displayId: number, edge: DispEdge | undefined, depth: number) => {
      rows.push({ kind: "node", id: displayId, edge, depth });
      if (!expanded.has(displayId)) return;
      const { edges, hidden } = shownDisplay(recurseId);
      for (const e of edges) walk(e.childId, e.displayId, e, depth + 1);
      if (hidden > 0) rows.push({ kind: "more", key: `more-${displayId}`, depth: depth + 1, count: hidden });
    };
    if (byId.has(rootId)) walk(rootId, rootId, undefined, 0);
    return rows;
  }

  const sel = byId.get(selected);
  const cPuct = tree?.c_puct ?? Math.SQRT2;
  // UCT exploration term for the selected node vs. its parent.
  const selU = (() => {
    if (!sel || sel.parent == null) return null;
    const parent = byId.get(sel.parent);
    if (!parent || sel.visits === 0) return null;
    return cPuct * Math.sqrt(Math.log(Math.max(2, parent.visits)) / sel.visits);
  })();

  // Human-readable explanation of every stat on a node, for the hover card.
  function tooltipContent(id: number) {
    const n = byId.get(id);
    if (!n) return null;
    const sharePct = Math.round((n.visits / Math.max(1, rootVisits)) * 100);
    const sm = n.summary;
    const Row = ({ k, v, hint }: { k: string; v: ReactNode; hint?: string }) => (
      <div style={{ display: "flex", gap: 8, justifyContent: "space-between" }}>
        <span style={{ color: "var(--muted)" }}>{k}</span>
        <span style={{ textAlign: "right" }}>{v}{hint && <span style={{ color: "var(--muted)" }}> {hint}</span>}</span>
      </div>
    );
    const chance = isChance(n);
    const spread = chance ? outcomeSpread(n) : null;
    const dpw = chance ? dpwInfo(n) : null;
    const prob = !chance ? outcomeProb(n) : null; // p for an outcome child
    const fmt = (q: number) => `${q >= 0 ? "+" : ""}${q.toFixed(3)}`;
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        <div style={{ fontWeight: 700, marginBottom: 2 }}>
          {chance && <span aria-hidden>🎲 </span>}
          {id === rootId ? "Current position (root)" : labelOf(n)}
          {n.target_index != null && <span style={{ color: "var(--muted)" }}> → {enemyName(n.target_index)}</span>}
        </div>
        {chance && <div style={{ color: "var(--muted)" }}>chance node — nature resolves a roll</div>}
        <Row k="Visits" v={n.visits} hint={`(${sharePct}% of search)`} />
        {chance ? (
          <Row k="Expectimax E[V]" v={fmt(n.q)} hint="Σ p·V over outcomes" />
        ) : (
          <Row k="Win value (Q)" v={fmt(n.q)} hint="−1 loss … +1 win" />
        )}
        {prob != null && <Row k="Outcome prob p" v={`.${Math.round(prob * 100).toString().padStart(2, "0")}`} hint="visits / chance visits" />}
        {spread && <Row k="Outcome spread" v={`${fmt(spread.min)} … ${fmt(spread.max)}`} hint="min … max V" />}
        {dpw && <Row k="Outcomes" v={`${dpw.k} / ${dpw.cap ?? "?"}`} hint={dpw.closed ? "closed" : dpw.truncated ? "truncated" : "(DPW cap)"} />}
        {dpw && (
          <div style={{ color: "var(--muted)" }}>
            Sampled {dpw.k} of ≥{dpw.k} outcomes{dpw.cap != null && ` (DPW cap ${dpw.cap})`}
            {dpw.closed ? "; distribution closed" : dpw.truncated ? "; distribution truncated" : ""}
            {n.chance_draws != null && ` · ${n.chance_draws} draws`}
          </div>
        )}
        <Row k="Playouts W/T/L" v={`${n.wins} / ${n.ties} / ${n.losses}`} />
        {n.proven_win && <div style={{ color: "#3ad17a" }}>✓ Forced win (solved)</div>}
        {n.proven_loss && <div style={{ color: "#e0566f" }}>✗ Forced loss</div>}
        {n.is_terminal && <div style={{ color: "var(--muted)" }}>Combat ends here</div>}
        {sm && (
          <>
            <div style={{ borderTop: "1px solid var(--border)", margin: "4px 0 2px", paddingTop: 4, color: "var(--muted)" }}>
              {id === rootId ? "Current state:" : "After this action:"}
            </div>
            <Row k="Player HP" v={`${sm.player_hp} / ${sm.player_max_hp}`} />
            {sm.player_block > 0 && <Row k="Block" v={sm.player_block} />}
            <Row k="Energy" v={sm.energy} />
            <Row k="Round" v={sm.round} />
            {sm.enemies.map((e, i) => (
              <Row key={e.uid} k={`Enemy ${i} HP`} v={`${e.hp} / ${e.max_hp}`} />
            ))}
          </>
        )}
      </div>
    );
  }
  const hoverHandlers = (id: number) => ({
    onMouseEnter: (e: ReactMouseEvent) => setHover({ id, x: e.clientX, y: e.clientY }),
    onMouseMove: (e: ReactMouseEvent) => setHover({ id, x: e.clientX, y: e.clientY }),
    onMouseLeave: () => setHover((h) => (h?.id === id ? null : h)),
  });

  return (
    <div role="dialog" aria-label="Search tree" onClick={onClose}
      style={{ position: "fixed", inset: 0, zIndex: 60, background: "oklch(20% 0.02 255 / .55)",
        display: "flex", alignItems: "center", justifyContent: "center", padding: 24 }}>
      <div onClick={(e) => e.stopPropagation()}
        style={{ background: "var(--bg)", border: "1px solid var(--border)", borderRadius: 10,
          width: "min(1500px, 97vw)", height: "94vh", display: "flex", flexDirection: "column",
          boxShadow: "0 12px 40px oklch(30% 0.04 255 / .4)" }}>
        {/* header / controls */}
        <div className="mono" style={{ display: "flex", alignItems: "center", gap: 14, flexWrap: "wrap",
          padding: "10px 16px", borderBottom: "1px solid var(--border)", fontSize: 12 }}>
          <b>MCTS search tree</b>
          <span style={{ display: "inline-flex", border: "1px solid var(--border)", borderRadius: 4, overflow: "hidden" }}>
            {(["outline", "graph"] as const).map((v) => (
              <button key={v} onClick={() => setView(v)}
                style={{ padding: "2px 9px", border: "none", cursor: "pointer", font: "inherit",
                  background: view === v ? "var(--accent, #6aa0ff)" : "transparent",
                  color: view === v ? "#fff" : "var(--text)" }}>{v}</button>
            ))}
          </span>
          <label title="number of MCTS playouts — more = stronger but slower">sims{" "}
            <select value={draft.sims} disabled={busy} onChange={(e) => setK("sims", Number(e.target.value))}>
              {SIM_CHOICES.map((s) => <option key={s} value={s}>{s}</option>)}
            </select>
          </label>
          <label title="UCT exploration constant — higher explores more, lower exploits the best line">explore{" "}
            <NumberInput min={0} step={0.1} value={draft.c_puct} disabled={busy} width={68}
              onKeyDown={(e) => e.key === "Enter" && dirty && run()}
              onChange={(v) => setK("c_puct", v)} />
          </label>
          <label title="rollout simulation depth in plies — how far each random playout looks ahead">depth{" "}
            <NumberInput min={1} max={500} value={draft.depth} disabled={busy} width={72}
              onKeyDown={(e) => e.key === "Enter" && dirty && run()}
              onChange={(v) => setK("depth", v)} />
          </label>
          <label title="rollout policy — heuristic (expert priorities) or uniform (unbiased random)">rollout{" "}
            <select value={draft.rollout_policy} disabled={busy}
              onChange={(e) => setK("rollout_policy", e.target.value as MctsConfig["rollout_policy"])}>
              <option value="heuristic">heuristic</option>
              <option value="uniform">uniform</option>
            </select>
          </label>
          <label title="RNG seed — fixes the determinization draw; change to re-roll">seed{" "}
            <NumberInput min={0} value={draft.seed} disabled={busy} width={72}
              onKeyDown={(e) => e.key === "Enter" && dirty && run()}
              onChange={(v) => setK("seed", v)} />
          </label>
          <label title="prune move orders the engine proves dominated — e.g. never search Strike-then-Beam-Cell when Beam-Cell-then-Strike is the same turn with more damage">
            <input type="checkbox" checked={draft.dominance_pruning ?? false} disabled={busy}
              onChange={(e) => setK("dominance_pruning", e.target.checked)} />{" "}prune
          </label>
          <button onClick={() => setK("seed", draft.seed + 1)} disabled={busy} title="re-roll: bump the seed"
            style={{ padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 4, cursor: "pointer" }}>🎲</button>
          <button onClick={run} disabled={busy} title="run the search with these parameters"
            style={{ padding: "2px 12px", borderRadius: 4, cursor: busy ? "wait" : "pointer", fontWeight: 600,
              border: `1px solid ${dirty ? "var(--accent, #6aa0ff)" : "var(--border)"}`,
              background: dirty ? "var(--accent, #6aa0ff)" : "transparent",
              color: dirty ? "#fff" : "var(--text)" }}>
            {busy ? "running…" : dirty ? "run ▶" : "rerun"}
          </button>
          <label>top-K{" "}
            <NumberInput min={1} max={12} value={topK} width={60}
              onChange={(v) => setTopK(v)} />
          </label>
          <label>min visits{" "}
            <NumberInput min={1} value={minVisits} width={68}
              onChange={(v) => setMinVisits(v)} />
          </label>
          <span style={{ display: "inline-flex", gap: 4 }}>
            <button onClick={() => expandBestLine(selected, 10)}
              title="unfold only the best (most-visited) line from the selected node, 10 levels deep"
              style={{ padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 4, cursor: "pointer" }}>best +10</button>
            <button onClick={expandAll} title="expand every node"
              style={{ padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 4, cursor: "pointer" }}>expand all</button>
            <button onClick={collapseAll} title="collapse to the root"
              style={{ padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 4, cursor: "pointer" }}>collapse</button>
            <button onClick={() => setCollapseChance((v) => !v)}
              title="fold the chance layer so a decision connects straight to its modal outcome through its action — the tree then reads like the legacy PIMC move line (value stays expectimax E[V])"
              style={{ padding: "2px 8px", borderRadius: 4, cursor: "pointer",
                border: `1px solid ${collapseChance ? "var(--accent, #6aa0ff)" : "var(--border)"}`,
                background: collapseChance ? "var(--accent, #6aa0ff)" : "transparent",
                color: collapseChance ? "#fff" : "var(--text)" }}>🎲 collapse dice</button>
          </span>
          {view === "graph" && (
            <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
              <button onClick={() => zoomBy(1 / 1.25)} aria-label="zoom out" title="zoom out"
                style={{ padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 4, cursor: "pointer" }}>−</button>
              <span style={{ width: 38, textAlign: "center", color: "var(--muted)" }}>{Math.round(scale * 100)}%</span>
              <button onClick={() => zoomBy(1.25)} aria-label="zoom in" title="zoom in"
                style={{ padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 4, cursor: "pointer" }}>＋</button>
              <button onClick={() => setZoom("fit")} title="fit the whole tree in view"
                style={{ padding: "2px 8px", border: `1px solid ${zoom === "fit" ? "var(--accent, #6aa0ff)" : "var(--border)"}`,
                  borderRadius: 4, cursor: "pointer" }}>fit</button>
            </span>
          )}
          <span style={{ color: "var(--muted)", marginLeft: "auto" }}>
            ●decision ◆chance · color ∝ value · gold = most-visited line · click to expand
          </span>
          <button onClick={onClose} aria-label="Close"
            style={{ padding: "4px 10px", border: "1px solid var(--border)", borderRadius: 4, cursor: "pointer" }}>
            close (Esc)
          </button>
        </div>

        {/* graph */}
        <div ref={wrapRef} style={{ overflow: "auto", flex: 1, position: "relative", opacity: busy ? 0.5 : 1 }}>
          {tree?.error ? (
            <div className="mono" style={{ padding: 24, color: "var(--hp)" }}>⚠ {tree.error}</div>
          ) : !built || !tree || tree.nodes.length === 0 ? (
            <div className="mono" style={{ padding: 24, color: "var(--muted)" }}>
              {busy ? "searching…" : "no tree"}
            </div>
          ) : view === "outline" ? (
            <div className="mono" style={{ fontSize: 12 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "3px 12px 3px 4px",
                position: "sticky", top: 0, zIndex: 1, background: "var(--bg)",
                borderBottom: "1px solid var(--border)", color: "var(--muted)", fontSize: 10,
                textTransform: "uppercase", letterSpacing: 0.4 }}>
                <span style={{ width: 12, flexShrink: 0 }} />
                <span style={{ flex: 1 }}>action</span>
                <span style={{ width: 70, textAlign: "right" }} title="how many times the search visited this node (more = more confidence)">visits</span>
                <span style={{ width: 64, textAlign: "right" }} title="expected value Q: +1 = sure win, 0 = even, −1 = sure loss">Q</span>
                <span style={{ width: 90 }} title="this node's share of all search visits">search&nbsp;%</span>
                <span style={{ width: 188 }} title="outcome after this action — ♥ player HP (+block) · ⚡ energy · r = round number · then each enemy's HP">outcome</span>
              </div>
              {outlineRows().map((r) =>
                r.kind === "more" ? (
                  <div key={r.key} style={{ paddingLeft: r.depth * 16 + 26, color: "var(--muted)", fontSize: 11, padding: "1px 0" }}>
                    ＋{r.count} more (raise top-K)
                  </div>
                ) : (() => {
                  const n = byId.get(r.id)!;
                  const isSel = r.id === selected;
                  const onPv = pvSet.has(r.id);
                  const hasKids = displayChildren(recurseIdFor(r.id)).length > 0;
                  const open = expanded.has(r.id);
                  const share = Math.round((n.visits / Math.max(1, rootVisits)) * 100);
                  const chance = isChance(n);
                  const outcome = !!r.edge?.outcome;
                  const dpw = chance ? dpwInfo(n) : null;
                  const prob = outcome ? outcomeProb(n) : null;
                  return (
                    <div key={r.id} onClick={() => toggle(r.id)} {...hoverHandlers(r.id)}
                      style={{ display: "flex", alignItems: "center", gap: 8, cursor: "pointer",
                        paddingLeft: r.depth * 16 + 4, paddingRight: 12, height: 22,
                        borderLeft: `2px solid ${onPv ? "var(--accent, #d9a441)" : "transparent"}`,
                        fontStyle: chance || outcome ? "italic" : "normal",
                        background: isSel ? "var(--panel)" : "transparent" }}>
                      <span aria-hidden style={{ width: 12, color: "var(--muted)", flexShrink: 0 }}>
                        {hasKids ? (open ? "▾" : "▸") : ""}
                      </span>
                      <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                        color: chance || outcome ? "var(--muted)" : undefined,
                        fontWeight: isSel ? 700 : onPv ? 600 : 400 }}>
                        {outcome ? (
                          <>
                            <span aria-hidden>⤳ </span>
                            p={prob != null ? `.${Math.round(prob * 100).toString().padStart(2, "0")}` : "?"}
                            {n.outcome_ordinal != null && <span> ·#{n.outcome_ordinal}</span>}
                          </>
                        ) : (
                          <>
                            {chance && <span aria-hidden>🎲 </span>}
                            {labelOf(n)}
                            {r.edge?.viaChance != null && <span aria-hidden title="a chance roll was folded into this action"> 🎲</span>}
                            {dpw && (
                              <span style={{ color: "var(--muted)", fontStyle: "normal" }}> {dpw.k}/{dpw.cap ?? "?"}
                                {dpw.closed ? " closed" : dpw.truncated ? " trunc" : ""}</span>
                            )}
                            {n.target_index != null && <span style={{ color: "var(--muted)" }}> → {enemyName(n.target_index)}</span>}
                            {n.proven_win && <span style={{ color: "#3ad17a" }}> ✓</span>}
                            {n.proven_loss && <span style={{ color: "#e0566f" }}> ✗</span>}
                          </>
                        )}
                      </span>
                      <span style={{ width: 70, textAlign: "right", color: "var(--muted)", flexShrink: 0 }}>N {n.visits}</span>
                      <span style={{ width: 64, textAlign: "right", color: qColor(n.q), flexShrink: 0 }}>
                        {chance && <span style={{ color: "var(--muted)" }}>E</span>}{chance ? " " : ""}{n.q >= 0 ? "+" : ""}{n.q.toFixed(2)}
                      </span>
                      <span title={`${share}% of root visits`} style={{ width: 90, height: 7, flexShrink: 0,
                        background: "var(--border)", borderRadius: 3, overflow: "hidden" }}>
                        <span style={{ display: "block", height: "100%", width: `${share}%`, background: qColor(n.q) }} />
                      </span>
                      {n.summary && (
                        <span style={{ width: 188, flexShrink: 0, color: "var(--muted)", fontSize: 11,
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                          ♥{n.summary.player_hp}{n.summary.player_block > 0 ? `+${n.summary.player_block}` : ""}
                          {" ⚡"}{n.summary.energy} r{n.summary.round}
                          {n.summary.enemies.length > 0 && ` · ${n.summary.enemies.map((e) => e.hp).join("·")}`}
                        </span>
                      )}
                    </div>
                  );
                })(),
              )}
            </div>
          ) : (
            // The footprint wrapper takes the post-scale size so the scroll
            // area is correct; the inner layer is scaled via transform.
            <div style={{ width: built.width * scale, height: built.height * scale }}>
            <div style={{ position: "relative", width: built.width, height: built.height,
              transform: `scale(${scale})`, transformOrigin: "top left" }}>
              <svg width={built.width} height={built.height}
                style={{ position: "absolute", inset: 0 }}>
                {/* edges */}
                {[...built.childMap.entries()].flatMap(([pid, kids]) => {
                  const p = built.pos.get(pid);
                  const pn = byId.get(pid);
                  if (!p || !pn) return [];
                  return kids.map((cid) => {
                    const c = built.pos.get(cid);
                    const cn = byId.get(cid);
                    if (!c || !cn) return null;
                    const onPv = pvSet.has(pid) && pvSet.has(cid);
                    const w = 1 + 5 * (cn.visits / Math.max(1, pn.visits));
                    const my = (p.y + c.y) / 2;
                    return (
                      <path key={`${pid}-${cid}`} d={`M ${p.x} ${p.y} C ${p.x} ${my}, ${c.x} ${my}, ${c.x} ${c.y}`}
                        fill="none" stroke={onPv ? "var(--accent, #d9a441)" : "var(--border)"}
                        strokeWidth={onPv ? Math.max(2, w) : w} strokeOpacity={onPv ? 1 : 0.7} />
                    );
                  });
                })}
                {/* nodes: decisions are circles ●, chance nodes diamonds ◆ */}
                {[...built.pos.entries()].map(([id, p]) => {
                  const n = byId.get(id);
                  if (!n) return null;
                  const onPv = pvSet.has(id);
                  const isSel = id === selected;
                  const ring = n.proven_win ? "#3ad17a" : n.proven_loss ? "#e0566f" : onPv ? "var(--accent, #d9a441)" : "var(--border)";
                  const sw = isSel ? 3 : onPv || n.proven_win || n.proven_loss ? 2 : 1;
                  const label = id === rootId ? "root" : labelOf(n);
                  const common = {
                    fill: qColor(n.q), stroke: isSel ? "var(--text)" : ring, strokeWidth: sw,
                    style: { cursor: busy ? "wait" : ("pointer" as const) },
                    onClick: () => toggle(id), ...hoverHandlers(id),
                  };
                  if (isChance(n)) {
                    // A diamond = the same visit-sized radius rotated 45°.
                    const r = radius(n);
                    const h = r / Math.SQRT2; // half-side so corners reach r
                    return (
                      <rect key={id} x={p.x - h} y={p.y - h} width={2 * h} height={2 * h}
                        transform={`rotate(45 ${p.x} ${p.y})`} aria-label={label} {...common} />
                    );
                  }
                  return (
                    <circle key={id} cx={p.x} cy={p.y} r={radius(n)} aria-label={label} {...common} />
                  );
                })}
              </svg>
              {/* labels (non-interactive; clicks go to the circles) */}
              {[...built.pos.entries()].map(([id, p]) => {
                const n = byId.get(id);
                if (!n) return null;
                const hidden = built.hiddenMap.get(id) ?? 0;
                const collapsedKids = !expanded.has(id) && displayChildren(recurseIdFor(id)).length > 0;
                const chance = isChance(n);
                const edge = built.edgeMap.get(id);
                const outcome = !!edge?.outcome;
                const prob = outcome ? outcomeProb(n) : null;
                const labelText = outcome
                  ? `⤳ p=${prob != null ? `.${Math.round(prob * 100).toString().padStart(2, "0")}` : "?"}`
                  : chance ? `🎲 ${labelOf(n)}` : labelOf(n);
                return (
                  <div key={id} className="mono" style={{ position: "absolute",
                    left: p.x + MAX_R + 5, top: p.y, transform: "translateY(-50%)",
                    pointerEvents: "none", lineHeight: 1.15,
                    fontStyle: chance || outcome ? "italic" : "normal",
                    color: chance || outcome ? "var(--muted)" : undefined,
                    // Cap to the column so long names can't bleed into the next one.
                    maxWidth: COL_GAP - MAX_R - 12,
                    fontWeight: id === selected ? 700 : pvSet.has(id) ? 600 : 400 }}>
                    <div style={{ fontSize: 11, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                      {labelText}
                      {edge?.viaChance != null && <span aria-hidden> 🎲</span>}
                      {collapsedKids && <span style={{ color: "var(--muted)" }}> ▸</span>}
                    </div>
                    <div style={{ fontSize: 9.5, color: "var(--muted)", whiteSpace: "nowrap" }}>
                      N={n.visits} {chance ? "E" : "Q"}={n.q.toFixed(2)}
                      {hidden > 0 && <span> ＋{hidden}</span>}
                    </div>
                  </div>
                );
              })}
            </div>
            </div>
          )}
        </div>

        {/* detail strip */}
        {sel && !tree?.error && (
          <div className="mono" style={{ display: "flex", gap: 16, flexWrap: "wrap",
            padding: "8px 16px", borderTop: "1px solid var(--border)", fontSize: 11.5, background: "var(--panel)" }}>
            <b>{isChance(sel) && <span aria-hidden>🎲 </span>}{labelOf(sel)}</b>
            {sel.target_index != null && <span style={{ color: "var(--muted)" }}>→ {enemyName(sel.target_index)}</span>}
            <span>N <b>{sel.visits}</b></span>
            <span>{valueKey(sel)} <b style={{ color: qColor(sel.q) }}>{sel.q.toFixed(3)}</b></span>
            {isChance(sel) && (() => {
              const sp = outcomeSpread(sel);
              const dpw = dpwInfo(sel);
              return (
                <>
                  {sp && <span style={{ color: "var(--muted)" }} title="min … max V over sampled outcomes">spread {sp.min >= 0 ? "+" : ""}{sp.min.toFixed(2)} … {sp.max >= 0 ? "+" : ""}{sp.max.toFixed(2)}</span>}
                  <span style={{ color: "var(--muted)" }} title={`sampled ${dpw.k} of ≥${dpw.k} outcomes (DPW cap ${dpw.cap ?? "?"})${dpw.closed ? "; closed" : dpw.truncated ? "; truncated" : ""}`}>
                    {dpw.k}/{dpw.cap ?? "?"} outcomes{dpw.closed ? " · closed" : dpw.truncated ? " · truncated" : ""}
                  </span>
                </>
              );
            })()}
            <span style={{ color: "var(--muted)" }}>W/T/L {sel.wins}/{sel.ties}/{sel.losses}</span>
            {selU != null && <span style={{ color: "var(--muted)" }}>U {selU.toFixed(3)}</span>}
            <span style={{ color: "var(--muted)" }}>children {sel.children.length}</span>
            {sel.proven_win && <span style={{ color: "#3ad17a" }}>proven win</span>}
            {sel.proven_loss && <span style={{ color: "#e0566f" }}>proven loss</span>}
            {sel.is_terminal && <span style={{ color: "var(--muted)" }}>terminal</span>}
            {sel.summary && (
              <span style={{ marginLeft: "auto", display: "inline-flex", gap: 12, alignItems: "center" }}>
                <span title="player HP (+ block)">
                  hp <b style={{ color: "var(--hp)" }}>{sel.summary.player_hp}/{sel.summary.player_max_hp}</b>
                  {sel.summary.player_block > 0 && <span style={{ color: "var(--muted)" }}> +{sel.summary.player_block} blk</span>}
                </span>
                <span style={{ color: "var(--muted)" }}>energy {sel.summary.energy}</span>
                <span style={{ color: "var(--muted)" }}>round {sel.summary.round}</span>
                {sel.summary.enemies.length > 0 && (
                  <span style={{ color: "var(--muted)" }}>
                    enemies {sel.summary.enemies.map((e) => `${e.hp}/${e.max_hp}`).join(", ")}
                  </span>
                )}
              </span>
            )}
          </div>
        )}
      </div>

      {/* Rich hover card: everything about the hovered node in plain English. */}
      {hover && byId.has(hover.id) && (
        <div className="mono" style={{ position: "fixed", zIndex: 70, pointerEvents: "none",
          left: Math.min(hover.x + 14, window.innerWidth - 250),
          top: Math.min(hover.y + 14, window.innerHeight - 230),
          width: 232, padding: "8px 10px", fontSize: 11, lineHeight: 1.45,
          background: "var(--bg)", border: "1px solid var(--border)", borderRadius: 6,
          boxShadow: "0 8px 24px oklch(20% 0.03 255 / .5)" }}>
          {tooltipContent(hover.id)}
        </div>
      )}
    </div>
  );
}
