/**
 * The live-game bridge from the BROWSER build (GH #709).
 *
 * The Python workbench reaches the advisor's control plane (the SimBridge
 * mod's rewind lane: `GET /status`, `/tree`, `/snapshots`, `POST /restore`,
 * `/search_tree` on `127.0.0.1:8079`) through a same-origin proxy. The
 * browser-only build has no server, so the page calls the control plane
 * itself — a cross-origin fetch from `https://sts2-workbench.pages.dev` to a
 * loopback `http://` address. That clears the browser's gates only with two
 * opt-ins, both off by default:
 *
 *  - on the advisor: `--control-allow-origin https://sts2-workbench.pages.dev`
 *    (the mod's "Browser workbench origin" setting), which makes the control
 *    plane answer CORS + Private Network Access headers for that origin and
 *    no other;
 *  - in this page: the control plane's URL, stored here in `localStorage`
 *    (nothing is probed until the user enters one).
 *
 * Measured 2026-09-08 (docs/browser-workbench.md, "Live-game bridge"):
 * Firefox 151 then just works; Chromium 149 additionally asks the user for
 * the "local network" permission (its Local Network Access check) the first
 * time — the fetch waits on that prompt. `targetAddressSpace` is Chrome's
 * hint that a loopback target is intended.
 */

const KEY = "sts2.bridgeUrl";

/** What the dialog pre-fills: the advisor's default control-plane port. */
export const DEFAULT_BRIDGE_URL = "http://127.0.0.1:8079";

/** Normalise a user-entered control-plane URL: scheme required, no path. */
export function normalizeBridgeUrl(raw: string): string | null {
  const s = raw.trim();
  if (!s) return null;
  let url: URL;
  try {
    url = new URL(s.includes("://") ? s : `http://${s}`);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  return url.origin;
}

/** The configured control-plane origin, or null (the default). */
export function bridgeBase(): string | null {
  try {
    const v = localStorage.getItem(KEY);
    return v ? normalizeBridgeUrl(v) : null;
  } catch {
    return null;
  }
}

/** Remember (or forget, with null) the control-plane origin. */
export function setBridgeBase(url: string | null): void {
  try {
    if (url) localStorage.setItem(KEY, url);
    else localStorage.removeItem(KEY);
  } catch {
    /* private mode: the setting lives for this page load only */
  }
}

/** A request straight to the control plane (base must be configured). */
export function bridgeFetch(path: string, init?: RequestInit): Promise<Response> {
  const base = bridgeBase();
  if (!base) return Promise.reject(new Error("no live-game bridge configured"));
  // `targetAddressSpace` is Chrome's Local Network Access hint; unknown
  // RequestInit keys are ignored elsewhere.
  const extra = { targetAddressSpace: "loopback" } as unknown as RequestInit;
  return fetch(`${base}${path}`, { ...init, ...extra });
}
