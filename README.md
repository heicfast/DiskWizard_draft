# DiskWizard

**Bring your files back.** Data recovery for Windows — recover files from any disk, preview before recovery, and keep every drive healthy.

Rust core + **Tauri 2 (WebView2)** + **React/TypeScript**. No Electron, no browser bloat.

> **Session status (v0.1 — UI/UX build):** the complete premium interface is in place, driven by a **simulated core** that mirrors the reverse-engineered product architecture 1:1 (command surface, event names, phase machines, licensing gates). The production Rust engine lands next and swaps in behind the frozen frontend contract. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`worklog.md`](worklog.md).

## Highlights

- **Cherry-blossom design system** — the Blossom palette (blush cream surfaces, rose ink gradients, white cards) plus Light / Dark / Ember / Tide, all crossfading app-wide via View Transitions. The brand mark's tile background is painted from the live theme gradient — switch theme, the logo follows.
- **Recover Data** — local disks, external devices, quick actions (Recycle Bin / Desktop / Select Folder / Disk Image) and recent sessions; TRIM, BitLocker and lost-partition states surfaced on the cards.
- **Scan Results** — the scan runs *inside* the results page: live searching strip, progressive tree fill, pause/stop, category tabs, folders/types tree, virtualized file table with **recoverability indicators** (Excellent / Good / Poor), inline preview panel with the recovery-chances gauge, essential-vs-complete result modes (the ~65% noise reduction), and the full recovery funnel (destination picker with Local/Cloud tabs, enable-file-repair, space gates).
- **Disk Tools** — S.M.A.R.T. dashboard (free forever — the Monitor-tier analog): per-disk health verdicts, temperature, wear, and the full attribute table; SD Card Rescue, lost-partition guidance, BitLocker unlock and disk imaging as licensed rescue tools.
- **Cloud Recovery** — iCloud, Google Drive, Dropbox, OneDrive sign-in → scan → recover, with the no-records-kept trust copy.
- **Licensing (DiskGenie architecture)** — paid app: every capability except disk health requires activation. Postures (unlicensed / pro / grace / degraded), typed server-error copy, Pro status card, command-layer `ACTIVATION_REQUIRED` gates, `DW-XXXXX-XXXXX-XXXXX-XXXXX` keys.
- **Elevated + maximized** — the app requests administrator (raw-disk access; `requireAdministrator` manifest) and opens maximized by default.

## Develop

```bash
npm install
npm run dev            # browser mode — the TS mock backend drives the whole UI
npm run tauri dev      # desktop mode — the Rust mock-core serves the same commands
```

Browser mode auto-installs the mock backend (`src/mock`) behind a DEV gate — it never ships in a bundle. The same command surface is implemented in Rust (`src-tauri/src/commands/mock`) so packaged dev builds behave identically.

## Build & release

```bash
npm run tauri build               # NSIS installer + MSIX package
```

CI builds the real artifacts (same pipelines as DiskGenie):

| Workflow | Artifact |
|---|---|
| `ci.yml` | typecheck + unit tests + the Windows **NSIS** installer |
| `msix-build.yml` | Store-ready **MSIX** (unsigned; Partner Center signs on ingest) |
| `ui-screenshots.yml` | one named screenshot per tour step (`DISKWIZARD_TOUR=1`) |

## The agent/mock channel

`?tour=1` (browser) or `DISKWIZARD_TOUR=1` (packaged) runs the TourDriver: it cycles every page, phase and dialog state, stamping `DiskWizard · tNN-name` into the title — the screenshot harness polls the title and captures exactly one frame per step. `window.__DW_TOUR_STATE__` exposes the current step; `window.__DW_MOCK__.seedPro()` (browser-dev) seeds a Pro license.

## Layout

```
src/               React/TS UI (pages, shell, state stores, theme, mock backend)
src-tauri/         Tauri 2 shell + the Rust mock-core (this session)
  windows-app-manifest.xml   requireAdministrator (raw-disk access)
docs/              architecture, licensing, decisions
worklog.md         the build journal
```
