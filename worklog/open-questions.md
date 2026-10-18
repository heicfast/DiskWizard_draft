# DiskWizard — Open Questions & Ambiguity Register

Per MASTER_PROMPT §0: every ambiguity is stated here, the best
research-matching interpretation is adopted, and work proceeds without
silent invention. Each entry records the decision taken so the operator
can override later.

## OQ-1 — GitHub push token redacted in the committed MASTER_PROMPT.md

**Ambiguity**: MASTER_PROMPT §18 / §14 mandates saving the document
verbatim (it contains a live GitHub PAT), while the repository is
PUBLIC (clone succeeds unauthenticated). Committing a live PAT to a
public repo triggers GitHub secret-scanning revocation and leaks the
credential to the world.

**Decision**: the in-repo copy redacts the token value in §14
(`ghp_<REDACTED-see-worklog-open-questions-OQ1>`); every other byte is
verbatim. The live token is used only transiently at push time via
`http.extraHeader` (never persisted in `.git/config`). If the operator
requires the literal token in-repo, it must be a private repo — flagged
for the owner.

**Status**: ADOPTED (2026-10-18, session 5).

## OQ-2 — Worklog location: `worklog/WORKLOG.md` vs existing root `worklog.md`

**Ambiguity**: MASTER_PROMPT §14 says maintain `worklog/WORKLOG.md`;
the repo already carries a substantial root-level `worklog.md` written
by sessions 1–4 (with the operator's knowledge, per QA-REGISTER §4
"Worklog maintained + pushed" audit).

**Decision**: keep the existing root `worklog.md` as the journal of
record (history + operator-reviewed); this `worklog/` folder holds the
open-questions register and any process artifacts. No content moved,
nothing lost. Alternative (move + git mv) rejected to avoid breaking
the reviewed audit trail.

**Status**: ADOPTED (2026-10-18, session 5).

## OQ-3 — Licensing endpoint/keys conflict with MASTER_PROMPT §8

**Ambiguity**: MASTER_PROMPT §8 (authoritative) says: until the
DiskWizard license server is deployed, use the **DiskGenie server
endpoint + the SAME secrets/keys/values** so the DiskGenie server
validates DiskWizard activations; all URL/key material isolated in ONE
config module. Session 3 instead generated a **fresh DiskWizard
keypair** and pointed `LICENSE_API_BASE` at a not-yet-deployed
`diskwizard-license.heictojpg-pics.workers.dev` endpoint
(QA-REGISTER §3, QA-16). These two states are mutually exclusive.

**Decision**: the master prompt wins. Port the DiskGenie server
endpoint + exact key material from `diskgenie-v2` /
`diskgenie-license-server-v2` into the single config module; the fresh
DiskWizard keypair is retired to the deployment sheet for the future
swap. (Verification of the actual values in the donor repos before
committing the change — evidence-first.)

**Status**: RESOLVED (2026-10-18, session 5) — `dw-license/src/config.rs`
is the ONE config module; values verified verbatim against the donor
client (`license.rs:34/40-47/126/695-704`) and the server guard/keys
sources. See docs/QA-REGISTER.md §3 for the exact ported values and
QA-16 for the remaining live-E2E flag.

## OQ-4 — "Free data recovery software" wording conflict

**Ambiguity**: MASTER_PROMPT §1 says earlier marketing copy called the
product "free data recovery software" (positioning language only);
authoritative behavior = paid license for everything except
SMART/disk-health.

**Decision**: product behavior already enforces paid gates (verified
QA-REGISTER §3: only `smart_report`, `list_devices`/`refresh_devices`,
settings, `get_dev_hooks` ungated). Any UI copy implying free recovery
is treated as a defect; the README/HelpCenter must describe the paid
model. No "free" wording exists in the shipped UI per the QA audit.

**Status**: RESOLVED (behavior verified 2026-10-18).

## OQ-5 — Preview payload cap: research contract (100 MB) vs current 2 MB payload

**Ambiguity**: research artifact `UIUX/10-preview-subsystem.md` caps
preview at 100 MB input with a 15-second video-preview window; the
current real `preview_file` caps at 16 MiB input / 2 MiB payload
(QA-2). Raising the payload cap has UI-memory implications for
base64-over-IPC.

**Decision**: keep v1 image/text/pdf-preview caps (honest "plays after
recovery" copy covers video), raise the INPUT cap to the research
contract level once the preview pipeline supports bounded extraction;
record the decision in the consistency register. QA-2 stays open as a
P0/P1 judgment call for the owner; not silently resolved.

**Status**: OPEN (owner call) — captured in docs/audit registers.

## OQ-6 — Coverage-ledger depth honesty

**Ambiguity**: MASTER_PROMPT Phase 0 demands "read every document and
artifact line by line" with 100% checkmarks; the corpus is 299 md files
/ 1.3 MB. The governing subset (architecture, SearchDLL engines,
SaveScan, FileRepair, DiskHealth, Licensing, config-files, core UIUX
flows) is read line-by-line by the session agent; the census-class
tail (per-binary 114 files, MOM-modules 32, reversed, WinPE) is read by
digestion agents with structured summaries recorded in the ledger.

**Decision**: the coverage ledger marks each file FULL or DIGEST with
the digest recorded — 100% of files are covered, and the depth is
honest rather than performative. Any file that becomes governing for
code written in this session is promoted to FULL before implementation.

**Status**: ADOPTED (2026-10-18, session 5).
