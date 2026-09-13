import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import RegretReview from "./RegretReview";
import type { DecisionRegret, RegretReview as Review } from "../api";

function decision(over: Partial<DecisionRegret> = {}): DecisionRegret {
  return {
    idx: 4,
    turn: 3,
    floor: 2,
    act: 1,
    played_label: "Strike",
    played_win_prob: 0.58,
    best_label: "Defend",
    best_win_prob: 0.71,
    regret: 0.13,
    drop_pct: 13,
    player_rank: 2,
    n_actions: 5,
    visits: 800,
    matched_best: false,
    is_blunder: true,
    message: "",
    ...over,
  };
}

function review(over: Partial<Review> = {}): Review {
  const blunder = decision();
  return {
    decisions: [blunder],
    blunders: [blunder],
    summary: { total_decisions: 1, blunder_count: 1, cumulative_drop_pct: 13, worst_drop_pct: 13 },
    ...over,
  };
}

describe("RegretReview dock", () => {
  it("renders as a non-modal region, not a dialog", () => {
    render(<RegretReview review={review()} onClose={() => {}} />);
    // A bottom dock, not a modal dialog — nothing should claim role=dialog.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("region", { name: /regret review/i })).toBeInTheDocument();
  });

  it("clicking a decision seeks there WITHOUT closing the dock", () => {
    const onSeek = vi.fn();
    const onClose = vi.fn();
    render(<RegretReview review={review()} onSeek={onSeek} onClose={onClose} />);

    fireEvent.click(screen.getByText(/Strike/).closest("button")!);

    // Passes the whole decision (so the parent can seek by node_id, stable
    // across rewinds) — not just its idx.
    expect(onSeek).toHaveBeenCalledWith(expect.objectContaining({ idx: 4 }));
    expect(onClose).not.toHaveBeenCalled(); // dock stays open — the core fix
  });

  it("re-run button fires onRerun without closing", () => {
    const onRerun = vi.fn();
    const onClose = vi.fn();
    render(<RegretReview review={review()} onRerun={onRerun} onClose={onClose} />);

    fireEvent.click(screen.getByRole("button", { name: /re-run/i }));

    expect(onRerun).toHaveBeenCalledOnce();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("hides the re-run button when no onRerun is provided", () => {
    render(<RegretReview review={review()} onClose={() => {}} />);
    expect(screen.queryByRole("button", { name: /re-run/i })).toBeNull();
  });

  it("closes on the ✕ button and on Escape", () => {
    const onClose = vi.fn();
    render(<RegretReview review={review()} onClose={onClose} />);

    fireEvent.click(screen.getByRole("button", { name: /close/i }));
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("shows the analyzing placeholder until the review arrives", () => {
    const { rerender } = render(<RegretReview review={null} onClose={() => {}} />);
    expect(screen.getByText(/analyzing/i)).toBeInTheDocument();

    rerender(<RegretReview review={review()} onClose={() => {}} />);
    expect(screen.queryByText(/analyzing/i)).toBeNull();
    expect(screen.getByText(/−13 pts/)).toBeInTheDocument();
  });

  it("notes the live line in the header when reviewing a live run", () => {
    render(<RegretReview review={review()} live onClose={() => {}} />);
    expect(screen.getByText(/live run/i)).toBeInTheDocument();
  });
});
