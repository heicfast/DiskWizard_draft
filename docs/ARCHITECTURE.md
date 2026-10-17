# DiskWizard Architecture (v0.1 — UI session, simulated core)

This document maps the shipped UI onto the product architecture derived
from the reverse-engineering research (the `easeus-drw-research`
artifacts) and states the frozen frontend contract the production Rust
core must implement.

## 1. Stack

| Layer | Technology |
|---|---|
| Shell | Tauri 2 (WebView2), frameless window, Overlay titlebar style, `requireAdministrator` manifest, opens maximized |
| UI | React 18 + TypeScript (Vite), Zustand stores, Framer Motion, Lucide icons, hand-rolled CSS design system (no heavyweight UI kit) |
| Theming | Two-layer attribute system (`data-theme` / `data-scheme`), 5 palettes, View-Transition crossfades (borrowed from DiskGenie, Blossom default) |
| Core (this session) | **Mock** — TS backend for browser-dev, Rust mock-core for packaged builds; both implement the identical command surface |

## 2. The frozen IPC contract

Every command the UI can call (names, args, events) is the interface
between UI and core. The production engine implements exactly these:

### Commands
| Command | Args → Result | Gated |
|---|---|---|
| `list_devices` | → `{localDisks, externalDevices, diskImages}` | no |
| `refresh_devices` | → `{ok}` | no |
| `connect_demo_device` | → `{ok, deviceId}` (dev/demo) | no |
| `smart_report` | `deviceId` → SMART report (free tier) | **no** |
| `sdr_diagnose` | `deviceId` → access-restore result | yes |
| `start_scan` | `deviceId` → `{generation}` | yes |
| `start_folder_scan` | `label` → `{generation}` | yes |
| `start_disk_image_scan` | `imagePath` → `{generation}` | yes |
| `start_cloud_scan` | `driveId` → `{generation}` | yes |
| `pause_scan` / `resume_scan` / `stop_scan` | → `{ok}` | no (state ops) |
| `get_scan_progress` | → `ScanProgress \| null` | no |
| `get_scan_results` | `generation` → `ScanResultsSnapshot \| null` | no |
| `list_sessions` / `resume_session` | → sessions / `{generation}` | yes (resume) |
| `list_recover_destinations` | `sourceDeviceId?` → destinations | no |
| `start_recovery` | `fileCount, bytes, destinationName, enableRepair` | yes |
| `list_cloud_drives` / `cloud_sign_in` | → drives / `{ok, account}` | no / no |
| `license_status` / `activate_license` / `deactivate_license` / `validate_now` | → `LicenseStatus` | no |
| `license_sim_set` | `pro` → `LicenseStatus` (tour/CI) | no |
| `get_settings` / `set_settings` | → settings | no |
| `get_dev_hooks` | → tour flags | no |

### Events
| Event | Payload |
|---|---|
| `scan-progress` | `ScanProgress` (phase machine: preparing → quick → deep → organizing → done; paused/stopped) |
| `scan-done` | `{generation, targetName, stats, durationSec, stopped?}` |
| `recover-progress` / `recover-done` | `RecoverProgress` |
| `license-changed` | `LicenseStatus` |
| `devices-changed` | — |

### Licensing gates
Privileged commands throw `ACTIVATION_REQUIRED. …` when unlicensed or
degraded; the ipc layer intercepts the marker and raises the
activation flow. Disk health (`smart_report`) is the free tier.

## 3. The two mocks (this session)

1. **TS mock** (`src/mock/`) — browser-dev/test only (DEV-gated,
   never bundled). Drives Playwright/agent-browser passes.
2. **Rust mock-core** (`src-tauri/src/commands/mock/`) — packaged dev
   and v0.1 release builds behave identically to the browser mock
   (same data, timings, events) so demo builds are production-real.

Both share the realism contract: the reference device inventory
(C:/D:/E:/F:/G: byte-exact), the 124,532-file/18.6 GB reference scan,
quick→deep phase proportions, progressive result filling in 1/40
slices, auto-saved `.rsf`-named sessions, recoverability grading
distributions, and the licensing posture machine.

## 4. UI structure

```
Shell (TitleBar, Sidebar, boot: splash → elevation → app)
Pages: recover | results (scan-in-page) | tools | cloud | settings | help
Stores: devices, scan, results, license, settings, view
Theme: tokens.css + themes.ts + useTheme (View-Transition crossfade)
Mock:  src/mock/commands.ts (TS) ─┐
                                  ├─ same contract → src/lib/ipc.ts
       src-tauri/.../mock.rs (RS) ┘
```

Key decisions cross-linked from the research:
- Scan runs **inside** the results page (the CDetailPage
  architecture; progressive fill while scanning).
- The left tree carries the special carve branches (File Path Lost /
  Tags Extracted) alongside the category tree (File Types mode).
- Recoverability chips on rows + the preview gauge = the
  check-node grading pass visualized.
- Essential/Complete results modes = the ~65% noise-reduction
  feature ("users can switch to complete scan results at any time").
- Destination picker: Local/Cloud tabs, enable-file-repair (ON),
  source-partition block + insufficient-space gate.

## 5. Release

NSIS (currentUser, embedded WebView2 bootstrapper) + MSIX
(runFullTrust + internetClient, 10.0.19041+, Store-signed on ingest)
— the DiskGenie pipelines, unchanged.
