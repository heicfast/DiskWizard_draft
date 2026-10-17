/**
 * Mock found-file generator (DEV/TEST ONLY).
 *
 * Produces the realistic results tree the UI browses: directory
 * structure + category mix + recoverability grading + the special
 * RAW-carve branches (File Path Lost / File Name Lost / Tags
 * Extracted / Versions) from the reverse-engineered results-phase
 * architecture. Deterministic per (device, generation) so screenshots
 * and CI tours are stable.
 *
 * Scale contract: the generator emits a few thousand REAL rows (what
 * the user actually scrolls) while `stats` reports the full simulated
 * find (e.g. 124,532 files / 18.6 GB for the reference USB target) —
 * exactly how the product's progressive tree presents a long scan.
 */
import type {
  CategoryCount,
  FileCategory,
  FileNode,
  Recoverability,
  ScanResultsSnapshot,
  StorageDevice,
} from "../types";
import { CATEGORY_ORDER } from "../types";
import { makeRng } from "./data";
import { GB, MB } from "./data";

/* ── Word banks (realistic user content) ─────────────────────────────── */

const PHOTO_NAMES = [
  "Vacation Photo", "Family", "Sunset Over Lake", "Birthday Party", "IMG_2023",
  "DSC_0481", "wedding-ceremony", "Tokyo Street at Night", "Cherry Blossoms",
  "Mountain Hike", "Screenshot", "Dinner with Friends", "Graduation",
  "PXL_20231024", "New Year Fireworks", "Beach Day", "team-offsite",
  "Product Prototype", "Whiteboard Session", "Pet Portrait",
];
const PHOTO_EXT = ["jpg", "jpeg", "png", "heic", "webp", "gif", "tiff"];
const DOC_NAMES = [
  "Project Proposal", "Invoice", "Resume", "Budget 2026", "Meeting Notes",
  "Contract Draft", "Thesis Chapter", "Tax Return", "Lease Agreement",
  "Quarterly Report", "Presentation Deck", "Recipe Collection", "Budget",
  "Insurance Policy", "Cover Letter", "Business Plan", "Whiteboard Sketch",
];
const DOC_EXT = ["docx", "pdf", "xlsx", "pptx", "doc", "txt", "md", "csv"];
const VIDEO_NAMES = [
  "holiday-4k", "birthday-recap", "Screen Recording", "Gameplay", "Drone Flight",
  "DJI_0214", "wedding-highlights", "concert-clip", "timelapse-sunset",
  "MVI_1304", "interview-take3", "screen-capture-bug",
];
const VIDEO_EXT = ["mp4", "mov", "avi", "mkv", "m4v"];
const AUDIO_NAMES = [
  "Podcast Episode", "Voice Memo", "Demo Track", "Lecture Recording",
  "mixtape-final", "Interview", "VoiceNotes", "field-recording",
];
const AUDIO_EXT = ["mp3", "wav", "m4a", "flac", "aac"];
const ARCHIVE_NAMES = [
  "backup-2026", "Photos Archive", "project-export", "old-website",
  "tax-documents", "disk-image", "downloads-bundle", "design-assets",
];
const ARCHIVE_EXT = ["zip", "rar", "7z", "tar.gz", "iso"];
const FOLDER_NAMES = [
  "Photos", "Documents", "Videos", "Music", "Downloads", "Desktop",
  "Work", "Projects", "Backups", "Archives", "Camera Roll", "Screenshots",
  "Invoices 2026", "website-2026", "Personal", "Travel", "Design",
  "Exports", "Old Laptop", "Financial",
];
const APP_NAMES = ["Photoshop", "Lightroom", "Excel", "Word", "VS Code", "Figma", "Steam"];

/* ── Per-target profiles ─────────────────────────────────────────────── */

interface TargetProfile {
  /** Reported full-scale find (the scan ticker's headline numbers). */
  statFiles: number;
  statBytes: number;
  /** Real rows generated (browsable). */
  rows: number;
  /** Category mix weights. */
  mix: Partial<Record<FileCategory, number>>;
  /** Root path label. */
  root: string;
  durationSec: number;
}

const PROFILES: Record<string, TargetProfile> = {
  "usb-F": {
    statFiles: 124_532,
    statBytes: 18.6 * GB,
    rows: 2_560,
    mix: { images: 34, videos: 16, documents: 22, audio: 9, archives: 5, other: 14 },
    root: "USB Drive (F:)",
    durationSec: 756, // 12m 36s — the reference capture
  },
  "disk0-part2": {
    statFiles: 1_139_275,
    statBytes: 147.0 * GB,
    rows: 3_200,
    mix: { images: 18, videos: 12, documents: 26, audio: 8, archives: 7, other: 29 },
    root: "Windows (C:)",
    durationSec: 2_340,
  },
  "disk1-part1": {
    statFiles: 214_830,
    statBytes: 96.2 * GB,
    rows: 2_880,
    mix: { images: 22, videos: 24, documents: 20, audio: 10, archives: 8, other: 16 },
    root: "Data (D:)",
    durationSec: 1_580,
  },
  "disk1-part2": {
    statFiles: 41_268,
    statBytes: 12.4 * GB,
    rows: 2_200,
    mix: { images: 10, videos: 6, documents: 44, audio: 4, archives: 12, other: 24 },
    root: "Projects (E:)",
    durationSec: 720,
  },
  "ssd-G": {
    statFiles: 189_004,
    statBytes: 141.7 * GB,
    rows: 2_400,
    mix: { images: 24, videos: 28, documents: 18, audio: 8, archives: 10, other: 12 },
    root: "Portable SSD (G:)",
    durationSec: 1_920,
  },
};

function profileFor(device: StorageDevice): TargetProfile {
  return (
    PROFILES[device.id] ?? {
      statFiles: 18_420,
      statBytes: 6.8 * GB,
      rows: 1_800,
      mix: { images: 26, videos: 14, documents: 24, audio: 8, archives: 8, other: 20 },
      root: device.name,
      durationSec: 640,
    }
  );
}

/* ── Generation ──────────────────────────────────────────────────────── */

interface GenRow {
  name: string;
  ext: string;
  category: FileCategory;
  size: number;
  folder: string;
  modified: number;
  origin: FileNode["origin"];
  recoverability: Recoverability;
  dims?: [number, number];
}

function pick<T>(rng: () => number, arr: T[]): T {
  return arr[Math.floor(rng() * arr.length) % arr.length];
}

function sizeFor(category: FileCategory, rng: () => number): number {
  const r = rng();
  switch (category) {
    case "images":
      return Math.round((0.4 + r * 7.6) * MB);
    case "videos":
      return Math.round((24 + r * 940) * MB);
    case "audio":
      return Math.round((2.2 + r * 9.5) * MB);
    case "documents":
      return Math.round((0.03 + r * 3.4) * MB);
    case "archives":
      return Math.round((3 + r * 260) * MB);
    default:
      return Math.round((0.01 + r * 42) * MB);
  }
}

/** Deterministic date within the last ~3 years, weighted recent. */
function dateFor(rng: () => number): number {
  const daysBack = Math.floor(Math.pow(rng(), 1.8) * 1_090); // ~3y, recent-heavy
  return Math.floor(Date.now() / 1000) - daysBack * 86400 - Math.floor(rng() * 86400);
}

function gradeFor(rng: () => number, origin: FileNode["origin"]): Recoverability {
  const r = rng();
  if (origin === "existing") return r < 0.82 ? "excellent" : r < 0.97 ? "good" : "poor";
  if (origin === "deleted") return r < 0.55 ? "excellent" : r < 0.88 ? "good" : "poor";
  if (origin === "tags") return r < 0.6 ? "excellent" : "good";
  return r < 0.35 ? "good" : r < 0.85 ? "good" : "poor";
}

/** 4032×3024-class phone-camera dimensions for a share of photos. */
function dimsFor(category: FileCategory, rng: () => number): [number, number] | undefined {
  if (category !== "images") return undefined;
  if (rng() > 0.72) return undefined;
  const sets: [number, number][] = [
    [4032, 3024], [3024, 4032], [6000, 4000], [1920, 1080], [3456, 4608], [5472, 3648],
  ];
  return pick(rng, sets);
}

function originFor(rng: () => number): FileNode["origin"] {
  const r = rng();
  if (r < 0.52) return "existing";
  if (r < 0.82) return "deleted";
  if (r < 0.93) return "lost-path";
  if (r < 0.965) return "lost-name";
  if (r < 0.99) return "tags";
  return "version";
}

function nameFor(category: FileCategory, rng: () => number): { name: string; ext: string } {
  switch (category) {
    case "images": {
      const ext = pick(rng, PHOTO_EXT);
      const base = pick(rng, PHOTO_NAMES);
      const suffix = ext === "heic" || ext === "jpg" ? (rng() < 0.5 ? `_${1000 + Math.floor(rng() * 8999)}` : "") : "";
      return { name: `${base}${suffix}`, ext };
    }
    case "videos":
      return { name: pick(rng, VIDEO_NAMES), ext: pick(rng, VIDEO_EXT) };
    case "audio":
      return { name: pick(rng, AUDIO_NAMES), ext: pick(rng, AUDIO_EXT) };
    case "documents": {
      const ext = pick(rng, DOC_EXT);
      const base = pick(rng, DOC_NAMES);
      return { name: rng() < 0.4 ? `${base} ${2024 + Math.floor(rng() * 3)}` : base, ext };
    }
    case "archives":
      return { name: pick(rng, ARCHIVE_NAMES), ext: pick(rng, ARCHIVE_EXT) };
    case "email":
      return {
        name: pick(rng, [
          "Inbox", "Sent Mail", "Archive folder", "Local Folders",
          "Outlook Data File", "Personal Folders", "Mailbox - Alex",
          "Old Mail 2019", "backup-mail", "Deleted Items",
          "work-mailbox", "family-mailbox",
        ]),
        ext: pick(rng, ["pst", "pst", "pst", "ost", "dbx", "eml"]),
      };
    default: {
      const r = rng();
      if (r < 0.3) return { name: pick(rng, APP_NAMES), ext: "exe" };
      if (r < 0.5) return { name: "index", ext: "html" };
      if (r < 0.7) return { name: pick(rng, ["settings", "config", "backup", "db"]), ext: "db" };
      return { name: pick(rng, FOLDER_NAMES), ext: "dat" };
    }
  }
}

function folderFor(category: FileCategory, rng: () => number, root: string, origin: FileNode["origin"]): string {
  if (origin === "lost-path" || origin === "lost-name") return "File Path Lost";
  if (origin === "tags") return "Tags Extracted";
  const base = root.replace(/^[A-Za-z]:\\?/, "").trim();
  const prefix = base ? `${root}\\` : `${root}\\`;
  const sub = (() => {
    switch (category) {
      case "images": return pick(rng, ["Photos", "Camera Roll", "Pictures", "Screenshots", "Travel"]);
      case "videos": return pick(rng, ["Videos", "Movies", "Drone Footage"]);
      case "audio": return pick(rng, ["Music", "Voice Memos", "Podcasts"]);
      case "documents": return pick(rng, ["Documents", "Work", "Desktop", "Invoices 2026", "Projects"]);
      case "archives": return pick(rng, ["Backups", "Downloads", "Archives"]);
      default: return pick(rng, ["Downloads", "AppData", "Misc"]);
    }
  })();
  const deep = rng() < 0.22 ? `\\${pick(rng, FOLDER_NAMES)}` : "";
  return `${prefix}${sub}${deep}`;
}

/**
 * Generate the results snapshot for a completed scan of `device`.
 * `fraction` (0..1) enables the progressive-fill behavior — a scan at
 * 68% presents 68% of the tree with the live stat counters scaled.
 */
export function generateResults(
  device: StorageDevice,
  generation: number,
  fraction = 1,
  seedOffset = 0,
): ScanResultsSnapshot {
  const profile = profileFor(device);
  const rng = makeRng(hashString(device.id) + generation * 7919 + seedOffset);
  const totalRows = Math.max(64, Math.round(profile.rows * fraction));

  // Category weights → normalized.
  const weights = CATEGORY_ORDER.map((c) => profile.mix[c] ?? 4);
  const wSum = weights.reduce((a, b) => a + b, 0);

  const rows: GenRow[] = [];
  for (let i = 0; i < totalRows; i++) {
    // Category by weighted dice.
    let dice = rng() * wSum;
    let cat: FileCategory = "other";
    for (let c = 0; c < CATEGORY_ORDER.length; c++) {
      if (dice < weights[c]) {
        cat = CATEGORY_ORDER[c];
        break;
      }
      dice -= weights[c];
    }
    const origin = originFor(rng);
    let { name, ext } = nameFor(cat, rng);
    if (origin === "lost-name") {
      name = `${cat === "images" ? "IMG" : "FILE"}${String(10000 + i).slice(-5)}`;
      ext = cat === "images" ? "jpg" : "bin";
    } else if (origin === "version") {
      name = `${name} (v2)`;
    }
    rows.push({
      name,
      ext,
      category: cat,
      size: sizeFor(cat, rng),
      folder: folderFor(cat, rng, profile.root, origin),
      modified: dateFor(rng),
      origin,
      recoverability: gradeFor(rng, origin),
      dims: dimsFor(cat, rng),
    });
  }

  // Build the node list: directories synthesized from folder strings +
  // files under them. IDs: dir nodes first (compact), then files.
  const dirIndex = new Map<string, number>();
  const nodes: FileNode[] = [];
  const now = Math.floor(Date.now() / 1000);

  const dirNode = (path: string): number => {
    const existing = dirIndex.get(path);
    if (existing !== undefined) return existing;
    const parentPath = path.slice(0, Math.max(path.lastIndexOf("\\"), 0));
    const parent = path === profile.root ? null : dirNode(parentPath || profile.root);
    const id = nodes.length;
    nodes.push({
      id,
      parentId: parent,
      name: path === profile.root ? profile.root : path.slice(path.lastIndexOf("\\") + 1),
      isDir: true,
      category: "other",
      size: 0,
      modified: now,
      origin: "existing",
      recoverability: "excellent",
      path,
      ext: "",
    });
    dirIndex.set(path, id);
    return id;
  };
  dirNode(profile.root);
  dirNode(`${profile.root}\\File Path Lost`);
  dirNode(`${profile.root}\\Tags Extracted`);

  for (const r of rows) {
    const dirId = dirNode(r.folder);
    nodes.push({
      id: nodes.length,
      parentId: dirId,
      name: `${r.name}.${r.ext}`,
      isDir: false,
      category: r.category,
      size: r.size,
      modified: r.modified,
      origin: r.origin,
      recoverability: r.recoverability,
      path: `${r.folder}\\${r.name}.${r.ext}`,
      width: r.dims?.[0],
      height: r.dims?.[1],
      deletedAgo: r.origin === "deleted" ? Math.max(0, Math.round((now - r.modified) / 86400)) : undefined,
      ext: r.ext,
    });
  }

  // Roll up dir sizes.
  for (let i = nodes.length - 1; i >= 0; i--) {
    const n = nodes[i];
    if (n.parentId !== null) nodes[n.parentId].size += n.size;
  }

  // Category counts: report at the SIMULATED scale (stats headline),
  // scaled by fraction so progressive fill reads consistently.
  const realCounts = new Map<FileCategory, { count: number; size: number }>();
  for (const n of nodes) {
    if (n.isDir) continue;
    const cur = realCounts.get(n.category) ?? { count: 0, size: 0 };
    cur.count += 1;
    cur.size += n.size;
    realCounts.set(n.category, cur);
  }
  const scale = (profile.statFiles * fraction) / Math.max(1, totalRows);
  const categories: CategoryCount[] = CATEGORY_ORDER.map((c) => {
    const real = realCounts.get(c) ?? { count: 0, size: 0 };
    return {
      category: c,
      count: Math.round(real.count * scale),
      size: Math.round((real.size / Math.max(1, totalRows)) * profile.statFiles * fraction),
    };
  });

  return {
    generation,
    targetName: profile.root,
    categories,
    nodes,
    durationSec: Math.round(profile.durationSec * fraction),
  };
}

function hashString(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}
