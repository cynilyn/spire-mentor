/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** `"static"` = browser-only workbench: every `/api/*` call is answered by
   *  the Rust engine (WASM) in a Web Worker instead of the Python backend.
   *  Unset = the classic dev / Cloud Run deployment against FastAPI. */
  readonly VITE_WORKBENCH_MODE?: string;
  /** Where the game art is served from in static mode (an origin or a path,
   *  trailing slash tolerated). Defaults to `/assets`, which the Vite dev /
   *  preview server mounts from the sibling sts2-wiki checkout. */
  readonly VITE_ASSETS_BASE?: string;
  /** Build identity (`git describe --always --dirty`, set by `just
   *  workbench-static`): the cache-buster on the VFX manifest / bindings
   *  fetched from the immutable art bucket, and the service worker's shell
   *  cache name (public/sw.js, stamped by vite.config.ts). Unset in plain
   *  `npm run build` / dev: the VFX fetch then falls back to Date.now(). */
  readonly VITE_BUILD_ID?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
