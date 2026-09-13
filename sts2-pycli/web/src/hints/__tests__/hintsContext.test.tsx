import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HintsProvider, useHints } from "../HintsContext";

function Probe() {
  const { on, toggle } = useHints();
  return (
    <button onClick={toggle} data-testid="probe">
      hints: {on ? "on" : "off"}
    </button>
  );
}

/** jsdom here exposes a `localStorage` object with no methods, so the suite
 *  supplies its own in-memory store — which is also what makes the persistence
 *  contract observable. */
function memoryStorage(seed: Record<string, string> = {}) {
  const map = new Map(Object.entries(seed));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
  };
}

beforeEach(() => vi.stubGlobal("localStorage", memoryStorage()));
afterEach(() => vi.unstubAllGlobals());

describe("HintsContext", () => {
  it("defaults to OFF — advice is never shown until asked for", () => {
    render(
      <HintsProvider>
        <Probe />
      </HintsProvider>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("hints: off");
  });

  it("defaults to OFF without a provider, so an ungated subtree cannot leak", () => {
    render(<Probe />);
    expect(screen.getByTestId("probe")).toHaveTextContent("hints: off");
  });

  it("toggling persists the choice across a remount", () => {
    const { unmount } = render(
      <HintsProvider>
        <Probe />
      </HintsProvider>,
    );
    fireEvent.click(screen.getByTestId("probe"));
    expect(screen.getByTestId("probe")).toHaveTextContent("hints: on");
    unmount();

    render(
      <HintsProvider>
        <Probe />
      </HintsProvider>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("hints: on");
  });

  it("reads a stored ON choice back as on", () => {
    vi.stubGlobal("localStorage", memoryStorage({ hintsOn: "1" }));
    render(
      <HintsProvider>
        <Probe />
      </HintsProvider>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("hints: on");
  });

  it("survives storage being unavailable (private mode) and stays off", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    });
    render(
      <HintsProvider>
        <Probe />
      </HintsProvider>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("hints: off");
    fireEvent.click(screen.getByTestId("probe"));
    expect(screen.getByTestId("probe")).toHaveTextContent("hints: on");
  });
});
