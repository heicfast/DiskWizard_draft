/**
 * The results-filter contract: visibility across the filter matrix and
 * the essential-vs-complete noise layer (the ~65% reduction note), plus
 * the store's filter/selection actions.
 */
import { beforeEach, describe, expect, it } from "vitest";
import { EMPTY_FILTER, filterIsActive, isVisible, useResultsStore } from "../results";
import type { FileNode } from "../../types";

const T = 1_700_000_000; // A fixed unix second (Nov 2023)

function file(over: Partial<FileNode> = {}): FileNode {
  return {
    id: 1,
    parentId: null,
    name: "photo.jpg",
    isDir: false,
    category: "images",
    size: 1024,
    modified: T,
    origin: "deleted",
    recoverability: "good",
    path: "F:\\Photos\\photo.jpg",
    ext: "jpg",
    ...over,
  };
}

describe("isVisible — filter matrix", () => {
  it("shows an unfiltered file in both modes", () => {
    const f = file();
    expect(isVisible(f, "essential", { ...EMPTY_FILTER })).toBe(true);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER })).toBe(true);
  });

  it("directories always pass (the tree folds them client-side)", () => {
    const dir = file({ isDir: true, origin: "lost-name" });
    expect(isVisible(dir, "essential", { ...EMPTY_FILTER })).toBe(true);
  });

  it("filters by category", () => {
    const f = file({ category: "videos" });
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, categories: ["videos"] })).toBe(true);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, categories: ["images", "audio"] })).toBe(
      false,
    );
  });

  it("filters by date range (inclusive bounds)", () => {
    const f = file({ modified: T });
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, dateFrom: T })).toBe(true);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, dateFrom: T + 1 })).toBe(false);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, dateTo: T })).toBe(true);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, dateTo: T - 1 })).toBe(false);
  });

  it("filters by size range (inclusive bounds)", () => {
    const f = file({ size: 2048 });
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, sizeMin: 2048 })).toBe(true);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, sizeMin: 2049 })).toBe(false);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, sizeMax: 2048 })).toBe(true);
    expect(isVisible(f, "complete", { ...EMPTY_FILTER, sizeMax: 2047 })).toBe(false);
  });

  it("existing-only keeps the existing origin alone", () => {
    expect(isVisible(file({ origin: "existing" }), "complete", { ...EMPTY_FILTER, existingOnly: true })).toBe(true);
    expect(isVisible(file({ origin: "deleted" }), "complete", { ...EMPTY_FILTER, existingOnly: true })).toBe(false);
    expect(isVisible(file({ origin: "tags" }), "complete", { ...EMPTY_FILTER, existingOnly: true })).toBe(false);
  });

  it("hide-empty drops 0-byte files", () => {
    expect(isVisible(file({ size: 0 }), "complete", { ...EMPTY_FILTER, hideEmpty: true })).toBe(false);
    expect(isVisible(file({ size: 1 }), "complete", { ...EMPTY_FILTER, hideEmpty: true })).toBe(true);
  });

  it("hide-junk drops system noise prefixes (case-insensitive)", () => {
    const junk = ["$LogFile", "pagefile.sys", "ntuser.dat", "UsnJrnl.j", "logo_big.png", "thumbs.db"];
    for (const name of junk) {
      expect(isVisible(file({ name }), "complete", { ...EMPTY_FILTER, hideJunk: true })).toBe(false);
    }
    expect(isVisible(file({ name: "Vacation.jpg" }), "complete", { ...EMPTY_FILTER, hideJunk: true })).toBe(true);
  });
});

describe("isVisible — essential vs complete (the ~65% noise layer)", () => {
  it("essential hides lost-name and version origins", () => {
    expect(isVisible(file({ origin: "lost-name" }), "essential", { ...EMPTY_FILTER })).toBe(false);
    expect(isVisible(file({ origin: "version" }), "essential", { ...EMPTY_FILTER })).toBe(false);
  });

  it("essential keeps the meaningful recovery origins", () => {
    for (const origin of ["existing", "deleted", "lost-path", "tags"] as const) {
      expect(isVisible(file({ origin }), "essential", { ...EMPTY_FILTER })).toBe(true);
    }
  });

  it("complete shows every origin", () => {
    for (const origin of ["existing", "deleted", "lost-path", "lost-name", "tags", "version"] as const) {
      expect(isVisible(file({ origin }), "complete", { ...EMPTY_FILTER })).toBe(true);
    }
  });
});

describe("filterIsActive", () => {
  it("is false for the empty filter", () => {
    expect(filterIsActive(EMPTY_FILTER)).toBe(false);
  });
  it("is true when any condition is set", () => {
    expect(filterIsActive({ ...EMPTY_FILTER, categories: ["images"] })).toBe(true);
    expect(filterIsActive({ ...EMPTY_FILTER, sizeMin: 1 })).toBe(true);
    expect(filterIsActive({ ...EMPTY_FILTER, dateTo: T })).toBe(true);
    expect(filterIsActive({ ...EMPTY_FILTER, hideJunk: true })).toBe(true);
  });
});

describe("results store — filter + selection actions", () => {
  beforeEach(() => {
    useResultsStore.getState().resetFilter();
    useResultsStore.getState().clearSelection();
    useResultsStore.setState({ search: "", resultsMode: "essential" });
  });

  it("patchFilter merges and resetFilter restores the empty filter", () => {
    useResultsStore.getState().patchFilter({ sizeMin: 1024, categories: ["images"] });
    const patched = useResultsStore.getState().filter;
    expect(patched.sizeMin).toBe(1024);
    expect(patched.categories).toEqual(["images"]);
    expect(filterIsActive(patched)).toBe(true);
    useResultsStore.getState().resetFilter();
    expect(useResultsStore.getState().filter).toEqual(EMPTY_FILTER);
  });

  it("toggleSelected flips membership and tracks the last anchor", () => {
    useResultsStore.getState().toggleSelected(7);
    expect(useResultsStore.getState().selectedIds.has(7)).toBe(true);
    expect(useResultsStore.getState().lastSelectedId).toBe(7);
    useResultsStore.getState().toggleSelected(7);
    expect(useResultsStore.getState().selectedIds.has(7)).toBe(false);
  });

  it("selectRange adds without clobbering; selectAll replaces; clearSelection empties", () => {
    useResultsStore.getState().toggleSelected(1);
    useResultsStore.getState().selectRange([2, 3]);
    expect(useResultsStore.getState().selectedIds.size).toBe(3);
    useResultsStore.getState().selectAll([10, 11]);
    expect([...useResultsStore.getState().selectedIds]).toEqual([10, 11]);
    useResultsStore.getState().clearSelection();
    expect(useResultsStore.getState().selectedIds.size).toBe(0);
  });

  it("setResultsMode flips the noise layer flag", () => {
    useResultsStore.getState().setResultsMode("complete");
    expect(useResultsStore.getState().resultsMode).toBe("complete");
    useResultsStore.getState().setResultsMode("essential");
    expect(useResultsStore.getState().resultsMode).toBe("essential");
  });
});
