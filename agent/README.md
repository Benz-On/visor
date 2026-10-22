# VISOR development agent — frozen

This Node server exists for exactly one purpose: letting the interface run in a
plain browser tab (`npm run agent` + `npm run dev`) while the Rust collector is
being developed or debugged. **It is not a second implementation of VISOR.**

## Status: frozen as a browser-preview stub

The agent is feature-frozen. New product capabilities — measured tok/s, GGUF
header metadata, session continuity, the native shell, alert history, price
settings — are implemented in the Rust collector only and will **not** be
back-ported here. Where a frozen behavior and the native collector differ,
the native collector is the product and this directory is the approximation.

The gaps are intentional and visible in the interface: no measured throughput
panel, no GGUF-exact model metadata, no session restore, and alert rule
toggles persist through the browser's `localStorage` rather than the settings
store.

## What is kept in sync on purpose

- `alert-engine.mjs` — the sustained-condition state machine and resolution
  records, so the record shape shown in a browser matches the native app.
- The embedded Windows collector scripts in `../src-tauri/collectors/` are
  shared source: both runtimes execute the same PowerShell.

## Maintenance rules

- Bug fixes that make the browser preview *wrong* (a crash, a fabricated
  value, a record shape that disagrees with the Rust engine) are in scope.
- Feature parity with the native collector is out of scope. If a feature
  needs to exist here to be tested, test it in the Rust suite instead.
- If the agent ever blocks a native change, the agent changes — or gets
  deleted. It must never gate the product.
