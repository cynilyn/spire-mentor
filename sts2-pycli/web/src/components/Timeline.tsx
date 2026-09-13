import type { TimelineEntry } from "../api";

const DECISION_COLOR: Record<string, string> = {
  combat_play: "var(--hp)",
  map_select: "var(--accent)",
  card_reward: "oklch(72% 0.13 145)",
  event_choice: "oklch(75% 0.13 85)",
  game_over: "var(--muted)",
};

export default function Timeline({ idx, count, entries, onSeek }: {
  idx: number; count: number; entries: TimelineEntry[]; onSeek: (idx: number) => void;
}) {
  const here = entries[idx];
  return (
    <div style={{ padding: "11px 18px", borderTop: "1px solid var(--border)" }}>
      <div style={{ display: "flex", gap: 6, marginBottom: 9, alignItems: "center" }}>
        <button className="mono" onClick={() => onSeek(0)} disabled={idx === 0}>⏮ first</button>
        <button className="mono" onClick={() => onSeek(idx - 1)} disabled={idx === 0}>◀ back</button>
        <button className="mono" onClick={() => onSeek(idx + 1)} disabled={idx >= count - 1}>fwd ▶▶</button>
        {here && (
          <span className="mono" style={{ marginLeft: "auto", fontSize: 11, color: "var(--text)" }}>
            {here.label || here.decision}
          </span>
        )}
      </div>
      {/* per-step band: one cell per decision, colored by type, current outlined */}
      <div style={{ display: "flex", gap: 1, height: 14, marginBottom: 6 }}>
        {entries.map((e) => (
          <div key={e.idx} title={`#${e.idx} ${e.decision}: ${e.label}`} onClick={() => onSeek(e.idx)}
            style={{ flex: 1, minWidth: 1, cursor: "pointer", borderRadius: 1,
              background: DECISION_COLOR[e.decision] ?? "var(--muted)",
              opacity: e.idx === idx ? 1 : 0.4,
              outline: e.idx === idx ? "2px solid var(--accent-strong)" : "none" }} />
        ))}
      </div>
      <input type="range" min={0} max={Math.max(0, count - 1)} value={idx}
        onChange={(e) => onSeek(Number(e.target.value))} style={{ width: "100%" }} />
      <div className="mono" style={{ fontSize: 10, color: "var(--muted)", marginTop: 6 }}>
        step {idx + 1} / {count}
        {here?.floor != null ? ` · ${here.act_name ?? `act ${here.act}`} floor ${here.floor}` : ""}
      </div>
    </div>
  );
}
