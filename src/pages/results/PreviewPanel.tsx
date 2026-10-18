/**
 * The preview panel (the reference design's right pane): content
 * preview, metadata block (type · size · modified · dimensions ·
 * path · status), the recovery-chances gauge (the CCheckNodeThread
 * grading visualized) and the Recover CTA.
 *
 * Real-core previews (CORE-INTERFACE-GAPS.md P0-2): files ≤ 16 MB
 * fetch their leading bytes via `preview_file` and render inline
 * (images as data URLs, text decoded, PDFs embedded). The render is
 * honest — when the bytes don't decode (damaged/truncated recoveries,
 * synthetic sim payloads) the visual falls back to the curated
 * samples/procedural art exactly like the browser-dev mock, so no
 * mode sniffing is needed and no broken frame ever shows.
 */
import { useEffect, useMemo, useState } from "react";
import { EyeIcon } from "lucide-react";
import { formatBytes, formatDateTime } from "../../lib/format";
import { invoke } from "../../lib/ipc";
import { useResultsStore } from "../../state/results";
import type { FileNode } from "../../types";
import { CategoryIcon, categoryTone } from "./categoryIcon";
import { GradeChip } from "./FileTable";
import vacationPhoto from "../../assets/previews/vacation-photo.jpg";
import familyPhoto from "../../assets/previews/family-photo.jpg";
import sakuraPhoto from "../../assets/previews/sakura-photo.jpg";
import mountainPhoto from "../../assets/previews/mountain-photo.jpg";

const PHOTO_MAP: { match: (name: string) => boolean; src: string }[] = [
  { match: (n) => n.startsWith("Vacation Photo"), src: vacationPhoto },
  { match: (n) => n.startsWith("Family"), src: familyPhoto },
  { match: (n) => /sakura|cherry/i.test(n), src: sakuraPhoto },
  { match: (n) => /mountain|hike|tokyo|beach/i.test(n), src: mountainPhoto },
];

/** The core caps preview payloads at 2 MB per call (files ≤ 16 MB). */
const PREVIEW_MAX_BYTES = 2_000_000;
const PREVIEW_SIZE_CEILING = 16 * 1024 * 1024;

type RealPreview = { mime: string; data: string };

/** Base64 → binary string (display only). */
function decodeB64(data: string): string {
  try {
    return atob(data);
  } catch {
    return "";
  }
}

/** Is this decodable as human text (≥ 90% printable in the head)? */
function looksLikeText(s: string): boolean {
  const head = s.slice(0, 2000);
  if (head.length === 0) return false;
  let printable = 0;
  for (const ch of head) {
    const c = ch.codePointAt(0) ?? 0;
    if (ch === "\n" || ch === "\t" || ch === "\r" || (c >= 32 && c <= 126) || c > 159) printable++;
  }
  return printable / head.length >= 0.9;
}

export function PreviewPanel() {
  const previewId = useResultsStore((s) => s.previewId);
  const snapshot = useResultsStore((s) => s.snapshot);
  const selectedIds = useResultsStore((s) => s.selectedIds);
  const openDestination = useResultsStore((s) => s.openDestination);
  const node = previewId !== null ? snapshot?.nodes[previewId] ?? null : null;

  // Real-core preview bytes (P0-2): fetch on selection; null while
  // pending and after failures — the fallback art IS the failure UX.
  const [real, setReal] = useState<RealPreview | null>(null);
  const [imgDecodeFailed, setImgDecodeFailed] = useState(false);
  const nodeId = node?.id ?? null;
  const generation = snapshot?.generation ?? 0;
  useEffect(() => {
    setReal(null);
    setImgDecodeFailed(false);
    if (nodeId === null || generation === 0) return;
    const n = snapshot?.nodes[nodeId];
    if (!n || n.isDir || n.size > PREVIEW_SIZE_CEILING) return;
    let alive = true;
    invoke<RealPreview & { size: number }>("preview_file", {
      generation,
      nodeId,
      maxBytes: PREVIEW_MAX_BYTES,
    })
      .then((r) => {
        if (alive) setReal({ mime: r.mime, data: r.data });
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [nodeId, generation, snapshot]);

  const realText = useMemo(() => {
    if (!real || !real.mime.startsWith("text/")) return null;
    const decoded = decodeB64(real.data);
    return looksLikeText(decoded) ? decoded.slice(0, 4000) : null;
  }, [real]);

  const selectedBytes = useMemo(() => {
    if (!snapshot) return 0;
    let b = 0;
    for (const id of selectedIds) {
      const n = snapshot.nodes[id];
      if (n) b += n.size;
    }
    return b;
  }, [snapshot, selectedIds]);

  if (!node) {
    return (
      <aside className="dw-preview-panel dw-preview-empty" aria-label="Preview">
        <div className="dw-preview-empty-art">
          <EyeIcon size={26} strokeWidth={1.5} />
        </div>
        <div className="dw-preview-empty-title">Preview</div>
        <p className="dw-preview-empty-sub">
          Double-click a file to preview it here — photos, videos, documents and
          archives render inline before you recover.
        </p>
      </aside>
    );
  }

  const isPhoto = node.category === "images";
  const realImage =
    real && real.mime.startsWith("image/") && !imgDecodeFailed
      ? `data:${real.mime};base64,${real.data}`
      : null;
  const photo = isPhoto ? PHOTO_MAP.find((p) => p.match(node.name))?.src : undefined;

  return (
    <aside className="dw-preview-panel dw-scroll" aria-label="Preview">
      <div className="dw-preview-stage">
        {isPhoto ? (
          <PhotoStage node={node} src={realImage ?? photo} onImgError={() => setImgDecodeFailed(true)} />
        ) : realText !== null ? (
          <figure className="dw-preview-doc">
            <pre className="dw-preview-text dw-scroll">{realText}</pre>
            <figcaption className="dw-preview-doc-cap">First {formatBytes(Math.min(real?.data.length ?? 0, PREVIEW_MAX_BYTES))} of the recovered bytes.</figcaption>
          </figure>
        ) : real && real.mime === "application/pdf" ? (
          <figure className="dw-preview-doc">
            <embed className="dw-preview-pdf" type="application/pdf" src={`data:application/pdf;base64,${real.data}`} />
            <figcaption className="dw-preview-doc-cap">Inline PDF preview (leading bytes).</figcaption>
          </figure>
        ) : (
          <DocStage node={node} />
        )}
      </div>

      <div className="dw-preview-meta">
        <div className="dw-preview-name dw-truncate" title={node.name}>{node.name}</div>
        <div className="dw-preview-kind">
          <span className={`dw-preview-kind-icon ${categoryTone(node.category)}`}>
            <CategoryIcon category={node.category} size={13} />
          </span>
          {node.ext.toUpperCase()} {categoryLabel(node)} · {formatBytes(node.size)}
        </div>

        <dl className="dw-preview-rows">
          <div className="dw-preview-row">
            <dt>Date modified</dt>
            <dd className="dw-num">{formatDateTime(node.modified)}</dd>
          </div>
          {node.width && node.height && (
            <div className="dw-preview-row">
              <dt>Dimensions</dt>
              <dd className="dw-num">{node.width} × {node.height}</dd>
            </div>
          )}
          <div className="dw-preview-row">
            <dt>Path</dt>
            <dd className="dw-preview-path" title={node.path}>{node.path}</dd>
          </div>
          <div className="dw-preview-row">
            <dt>Status</dt>
            <dd>
              <span className={`dw-chip ${node.origin === "existing" ? "dw-chip-ok" : "dw-chip-soft"}`}>
                {originLabel(node)}
              </span>
            </dd>
          </div>
          <div className="dw-preview-row">
            <dt>Recovery chances</dt>
            <dd className="dw-preview-grade">
              <span className={`dw-grade-ring dw-grade-ring-${node.recoverability}`} aria-hidden="true">
                <svg width="18" height="18" viewBox="0 0 18 18">
                  <circle cx="9" cy="9" r="7" fill="none" strokeWidth="2.4" className="dw-grade-ring-track" />
                  <circle cx="9" cy="9" r="7" fill="none" strokeWidth="2.4" strokeLinecap="round"
                    className={`dw-grade-ring-fill is-${node.recoverability}`}
                    strokeDasharray={2 * Math.PI * 7}
                    strokeDashoffset={2 * Math.PI * 7 * (1 - (node.recoverability === "excellent" ? 0.92 : node.recoverability === "good" ? 0.66 : 0.3))}
                    transform="rotate(-90 9 9)"
                  />
                </svg>
              </span>
              <GradeChip grade={node.recoverability} />
            </dd>
          </div>
        </dl>
      </div>

      <div className="dw-preview-cta">
        <button
          type="button"
          className="dw-ink-button dw-preview-recover"
          disabled={selectedIds.size === 0}
          onClick={() => void openDestination()}
        >
          Recover Selected Files
        </button>
        <div className="dw-preview-cta-note dw-num">
          {selectedIds.size > 0
            ? `${selectedIds.size} file${selectedIds.size === 1 ? "" : "s"} selected (${formatBytes(selectedBytes)})`
            : "Select files to recover"}
        </div>
      </div>
    </aside>
  );
}

function PhotoStage({ node, src, onImgError }: { node: FileNode; src?: string; onImgError?: () => void }) {
  return (
    <figure className="dw-preview-photo">
      {src ? (
        <img src={src} alt={node.name} draggable={false} onError={onImgError} />
      ) : (
        <SakuraArt />
      )}
    </figure>
  );
}

function DocStage({ node }: { node: FileNode }) {
  return (
    <figure className="dw-preview-doc">
      <div className="dw-preview-doc-card">
        <div className="dw-preview-doc-head">
          <span className={`dw-preview-kind-icon ${categoryTone(node.category)}`}>
            <CategoryIcon category={node.category} size={18} />
          </span>
          <span className="dw-preview-doc-ext">{node.ext.toUpperCase()}</span>
        </div>
        <div className="dw-preview-doc-lines" aria-hidden="true">
          <span className="is-title" />
          <span />
          <span />
          <span className="is-short" />
          <span />
          <span className="is-short" />
        </div>
      </div>
      <figcaption className="dw-preview-doc-cap">
        {node.category === "videos"
          ? "Video preview plays after recovery (15s preview in the free state)."
          : node.category === "audio"
            ? "Audio preview plays after recovery."
            : "Document preview renders after recovery."}
      </figcaption>
    </figure>
  );
}

/** Procedural sakura placeholder art for unmapped images. */
function SakuraArt() {
  return (
    <svg viewBox="0 0 320 240" className="dw-sakura-art" aria-hidden="true">
      <defs>
        <linearGradient id="dw-sky" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#fdeef0" />
          <stop offset="100%" stopColor="#f9dbe2" />
        </linearGradient>
        <linearGradient id="dw-mtn" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#cadede" />
          <stop offset="100%" stopColor="#a7c3cc" />
        </linearGradient>
      </defs>
      <rect width="320" height="240" fill="url(#dw-sky)" />
      <path d="M0 196 L64 132 L104 168 L150 116 L208 178 L246 148 L320 200 L320 240 L0 240 Z" fill="url(#dw-mtn)" opacity="0.85" />
      <path d="M0 214 L58 178 L128 208 L196 182 L262 212 L320 190 L320 240 L0 240 Z" fill="#f3cdd8" opacity="0.8" />
      {[
        [42, 52, 5, 0], [96, 30, 4, 20], [150, 66, 5.4, -14], [214, 38, 4.2, 30],
        [268, 72, 5, 8], [292, 30, 3.6, -20], [70, 96, 3.4, 40], [186, 104, 4, -30],
      ].map(([cx, cy, r, rot], i) => (
        <Petal key={i} cx={cx} cy={cy} r={r} rot={rot} />
      ))}
    </svg>
  );
}

function Petal({ cx, cy, r, rot }: { cx: number; cy: number; r: number; rot: number }) {
  return (
    <g transform={`translate(${cx} ${cy}) rotate(${rot}) scale(${r / 5})`}>
      <circle cx="0" cy="-3.4" r="2.1" fill="#f6aebb" />
      <circle cx="3.1" cy="-1.2" r="2.1" fill="#f2a0b4" />
      <circle cx="2" cy="2.6" r="2.1" fill="#ee96ac" />
      <circle cx="-2" cy="2.6" r="2.1" fill="#f2a0b4" />
      <circle cx="-3.1" cy="-1.2" r="2.1" fill="#f6aebb" />
      <circle cx="0" cy="0" r="1" fill="#ffd76e" />
    </g>
  );
}

function categoryLabel(n: FileNode): string {
  switch (n.category) {
    case "images": return "Image";
    case "videos": return "Video";
    case "audio": return "Audio";
    case "documents": return "Document";
    case "archives": return "Archive";
    case "email": return "Mail store";
    default: return "File";
  }
}

function originLabel(n: FileNode): string {
  switch (n.origin) {
    case "existing": return "Existing";
    case "deleted": return "Deleted";
    case "lost-path": return "Path lost";
    case "lost-name": return "Name lost";
    case "tags": return "Tagged";
    case "version": return "Older version";
    default: return "Found";
  }
}
