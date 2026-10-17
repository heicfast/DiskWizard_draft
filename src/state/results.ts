/**
 * Results store — the Scan Results page state: the browsable snapshot,
 * category/tab filtering, tree mode, search, the advanced filter
 * (type/date/size/toggles — the AdvanceFilterCondition analog),
 * selection, the preview target, essential-vs-complete result modes,
 * and the recovery funnel (destination → progress → summary).
 */
import { create } from "zustand";
import { invoke, listen } from "../lib/ipc";
import type {
  FileCategory,
  FileNode,
  NodeOrigin,
  RecoverDestination,
  ScanResultsSnapshot,
} from "../types";
import { EV_RECOVER_DONE, EV_RECOVER_PROGRESS } from "../types";

export type TreeMode = "folders" | "types";

export interface FilterCondition {
  /** Selected categories (empty = all). */
  categories: FileCategory[];
  /** Date range (unix seconds; null = open). */
  dateFrom: number | null;
  dateTo: number | null;
  /** Size range bytes (null = open). */
  sizeMin: number | null;
  sizeMax: number | null;
  /** Toggles: existing-only, hide-0KB, hide-system/temp junk. */
  existingOnly: boolean;
  hideEmpty: boolean;
  hideJunk: boolean;
}

export const EMPTY_FILTER: FilterCondition = {
  categories: [],
  dateFrom: null,
  dateTo: null,
  sizeMin: null,
  sizeMax: null,
  existingOnly: false,
  hideEmpty: false,
  hideJunk: false,
};

export function filterIsActive(f: FilterCondition): boolean {
  return (
    f.categories.length > 0 ||
    f.dateFrom !== null ||
    f.dateTo !== null ||
    f.sizeMin !== null ||
    f.sizeMax !== null ||
    f.existingOnly ||
    f.hideEmpty ||
    f.hideJunk
  );
}

export interface RecoverProgressState {
  totalFiles: number;
  doneFiles: number;
  failedFiles: number;
  totalBytes: number;
  doneBytes: number;
  currentFile: string;
  destination: string;
  enableRepair: boolean;
  repairedFiles: number;
  finished: boolean;
}

interface ResultsState {
  snapshot: ScanResultsSnapshot | null;
  loading: boolean;
  /* Browsing state */
  activeCategory: FileCategory | "all";
  treeMode: TreeMode;
  search: string;
  filter: FilterCondition;
  filterOpen: boolean;
  resultsMode: "essential" | "complete";
  /* Selection + preview */
  selectedIds: Set<number>;
  lastSelectedId: number | null;
  previewId: number | null;
  /* Recovery funnel */
  destinationOpen: boolean;
  destinations: RecoverDestination[];
  destinationsLoading: boolean;
  chosenDestinationId: string | null;
  enableRepair: boolean;
  recoverProgress: RecoverProgressState | null;
  recoverSummary: RecoverProgressState | null;
  /* Actions */
  load: (generation: number) => Promise<void>;
  setCategory: (c: FileCategory | "all") => void;
  setTreeMode: (m: TreeMode) => void;
  setSearch: (s: string) => void;
  patchFilter: (p: Partial<FilterCondition>) => void;
  resetFilter: () => void;
  setFilterOpen: (open: boolean) => void;
  setResultsMode: (mode: "essential" | "complete") => void;
  toggleSelected: (id: number) => void;
  selectRange: (ids: number[]) => void;
  selectAll: (ids: number[]) => void;
  clearSelection: () => void;
  openPreview: (id: number | null) => void;
  openDestination: () => Promise<void>;
  closeDestination: () => void;
  chooseDestination: (id: string) => void;
  setEnableRepair: (on: boolean) => void;
  beginRecovery: (nodeIds: number[], bytes: number) => Promise<void>;
  clearRecovery: () => void;
  dismissSummary: () => void;
}

export const useResultsStore = create<ResultsState>((set, get) => ({
  snapshot: null,
  loading: false,
  activeCategory: "all",
  treeMode: "folders",
  search: "",
  filter: { ...EMPTY_FILTER },
  filterOpen: false,
  resultsMode: "essential",
  selectedIds: new Set<number>(),
  lastSelectedId: null,
  previewId: null,
  destinationOpen: false,
  destinations: [],
  destinationsLoading: false,
  chosenDestinationId: null,
  enableRepair: true,
  recoverProgress: null,
  recoverSummary: null,

  load: async (generation) => {
    set({ loading: true });
    try {
      const snap = await invoke<ScanResultsSnapshot>("get_scan_results", { generation });
      set({ snapshot: snap ?? null, loading: false });
    } catch {
      set({ loading: false });
    }
  },

  setCategory: (c) => set({ activeCategory: c }),
  setTreeMode: (m) => set({ treeMode: m }),
  setSearch: (s) => set({ search: s }),
  patchFilter: (p) => set({ filter: { ...get().filter, ...p } }),
  resetFilter: () => set({ filter: { ...EMPTY_FILTER } }),
  setFilterOpen: (open) => set({ filterOpen: open }),
  setResultsMode: (mode) => set({ resultsMode: mode }),

  toggleSelected: (id) => {
    const sel = new Set(get().selectedIds);
    if (sel.has(id)) sel.delete(id);
    else sel.add(id);
    set({ selectedIds: sel, lastSelectedId: id });
  },
  selectRange: (ids) => {
    const sel = new Set(get().selectedIds);
    for (const id of ids) sel.add(id);
    set({ selectedIds: sel });
  },
  selectAll: (ids) => set({ selectedIds: new Set(ids) }),
  clearSelection: () => set({ selectedIds: new Set() }),
  openPreview: (id) => set({ previewId: id }),

  openDestination: async () => {
    set({ destinationOpen: true, destinationsLoading: true, chosenDestinationId: null });
    try {
      const sourceId = get().snapshot ? null : null;
      const list = await invoke<RecoverDestination[]>("list_recover_destinations", { sourceDeviceId: sourceId });
      set({ destinations: list, destinationsLoading: false });
    } catch {
      set({ destinationsLoading: false });
    }
  },
  closeDestination: () => set({ destinationOpen: false }),
  chooseDestination: (id) => set({ chosenDestinationId: id }),
  setEnableRepair: (on) => set({ enableRepair: on }),

  beginRecovery: async (nodeIds, bytes) => {
    const dest = get().destinations.find((d) => d.id === get().chosenDestinationId);
    set({ destinationOpen: false, recoverProgress: {
      totalFiles: nodeIds.length,
      doneFiles: 0,
      failedFiles: 0,
      totalBytes: bytes,
      doneBytes: 0,
      currentFile: "Preparing…",
      destination: dest?.name ?? "Destination",
      enableRepair: get().enableRepair,
      repairedFiles: 0,
      finished: false,
    } });
    await invoke("start_recovery", {
      generation: get().snapshot?.generation ?? 0,
      fileCount: nodeIds.length,
      bytes,
      destinationName: dest?.name ?? "Destination",
      enableRepair: get().enableRepair,
    }).catch(() => undefined);
  },

  clearRecovery: () => set({ recoverProgress: null }),
  dismissSummary: () => set({ recoverSummary: null }),
}));

let attached = false;
/** Attach recovery-progress event listeners. */
export function attachRecoverEvents(): void {
  if (attached) return;
  attached = true;
  void listen<RecoverProgressState>(EV_RECOVER_PROGRESS, (p) => {
    useResultsStore.setState({ recoverProgress: p });
  }).catch(() => undefined);
  void listen<RecoverProgressState>(EV_RECOVER_DONE, (p) => {
    const s = useResultsStore.getState();
    useResultsStore.setState({ recoverProgress: null, recoverSummary: p, selectedIds: new Set() });
    void s;
  }).catch(() => undefined);
}

/* ── Derived selectors (pure helpers for the page) ───────────────────── */

/** Origins hidden by the essential-results mode (the ~65% noise
 * reduction; "Users can switch to complete scan results at any time"). */
const ESSENTIAL_HIDDEN_ORIGINS: NodeOrigin[] = ["lost-name", "version"];

export interface JunkRule {
  test: (n: FileNode) => boolean;
}

const JUNK_PREFIXES = ["$", "pagefile", "ntuser", "UsnJrnl", "logo_", "thumb"];

export function isVisible(
  n: FileNode,
  mode: "essential" | "complete",
  filter: FilterCondition,
): boolean {
  if (n.isDir) return true;
  if (mode === "essential" && ESSENTIAL_HIDDEN_ORIGINS.includes(n.origin)) return false;
  if (filter.categories.length > 0 && !filter.categories.includes(n.category)) return false;
  if (filter.dateFrom !== null && n.modified < filter.dateFrom) return false;
  if (filter.dateTo !== null && n.modified > filter.dateTo) return false;
  if (filter.sizeMin !== null && n.size < filter.sizeMin) return false;
  if (filter.sizeMax !== null && n.size > filter.sizeMax) return false;
  if (filter.existingOnly && n.origin !== "existing") return false;
  if (filter.hideEmpty && n.size === 0) return false;
  if (filter.hideJunk) {
    const lower = n.name.toLowerCase();
    if (JUNK_PREFIXES.some((p) => lower.startsWith(p.toLowerCase()))) return false;
  }
  return true;
}
