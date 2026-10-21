/**
 * Tests for the UI/UX overhaul's new surface: the partition
 * reconstruction contract (mock invariants + store filtering) and the
 * realtime telemetry formatters.
 */
import { describe, expect, it } from "vitest";
import { formatRemaining, formatScanSpeed, formatSectors } from "../../lib/format";
import { generateResults } from "../../mock/files";
import { DEVICES } from "../../mock/data";
import { useResultsStore, isVisible, EMPTY_FILTER } from "../results";
import type { FileNode, StorageDevice } from "../../types";

const usb = DEVICES.externalDevices.find((d) => d.id === "usb-F") as StorageDevice;

/* ── formatSectors ───────────────────────────────────────────────────── */

describe("formatSectors", () => {
  it("comma-groups sector addresses", () => {
    expect(formatSectors(1_245_632)).toBe("1,245,632");
    expect(formatSectors(0)).toBe("0");
  });

  it("degrades on invalid input", () => {
    expect(formatSectors(Number.NaN)).toBe("—");
    expect(formatSectors(-5)).toBe("—");
  });
});

/* ── formatScanSpeed ─────────────────────────────────────────────────── */

describe("formatScanSpeed", () => {
  it("derives MB/s from 512-byte sectors over elapsed time", () => {
    // 2048 sectors × 512 B = 1 MiB over 1 s.
    expect(formatScanSpeed(2048, 1)).toBe("1 MB/s");
    // 20480 sectors × 512 B = 10 MiB over 2 s.
    expect(formatScanSpeed(20480, 2)).toBe("5 MB/s");
  });

  it("switches to GB/s beyond 1024 MB/s", () => {
    // 4 Mi sectors × 512 B = 2 GiB over 1 s.
    expect(formatScanSpeed(4 * 1024 * 1024, 1)).toBe("2.0 GB/s");
  });

  it("shows KB/s under 1 MB/s", () => {
    expect(formatScanSpeed(2, 1)).toBe("1 KB/s");
  });

  it("shows — before half a second (no stable rate yet)", () => {
    expect(formatScanSpeed(100_000, 0.2)).toBe("—");
    expect(formatScanSpeed(100_000, 0)).toBe("—");
    expect(formatScanSpeed(Number.NaN, 5)).toBe("—");
  });
});

/* ── formatRemaining (the 07m43s wire form) ──────────────────────────── */

describe("formatRemaining", () => {
  it("zero-pads minutes and seconds", () => {
    expect(formatRemaining(463)).toBe("07m43s");
    expect(formatRemaining(60)).toBe("01m00s");
  });

  it("clamps negatives to zero", () => {
    expect(formatRemaining(-3)).toBe("00m00s");
  });
});

/* ── The partition reconstruction contract (mock invariants) ─────────── */

describe("generateResults partition contract", () => {
  it("single-volume profiles report exactly one existing partition", () => {
    const c = DEVICES.localDisks.find((d) => d.id === "disk0-part2") as StorageDevice;
    const snap = generateResults(c, 1, 1);
    expect(snap.partitions).toHaveLength(1);
    expect(snap.partitions?.[0].isLost).toBe(false);
    expect(snap.partitions?.[0].fileCount).toBeGreaterThan(0);
  });

  it("the lost partition materializes only after LOST_AT", () => {
    const early = generateResults(usb, 1, 0.3);
    expect(early.partitions?.find((p) => p.isLost)).toBeUndefined();

    const late = generateResults(usb, 1, 0.8);
    const lost = late.partitions?.find((p) => p.isLost);
    expect(lost).toBeDefined();
    expect(lost!.fileCount).toBeGreaterThan(0);
  });

  it("every file node carries a partition id when the layout has one", () => {
    const snap = generateResults(usb, 1, 1);
    const files = snap.nodes.filter((n) => !n.isDir);
    expect(files.length).toBeGreaterThan(100);
    for (const f of files) {
      expect(f.partition === "p0" || f.partition === "p-lost").toBe(true);
    }
  });

  it("partition roll-ups cover every file exactly once", () => {
    const snap = generateResults(usb, 1, 1);
    const files = snap.nodes.filter((n) => !n.isDir);
    const byId = new Map((snap.partitions ?? []).map((p) => [p.id, p]));
    const realCount: Record<string, number> = {};
    for (const f of files) {
      const key = f.partition ?? "p0";
      realCount[key] = (realCount[key] ?? 0) + 1;
    }
    for (const [id, count] of Object.entries(realCount)) {
      expect(byId.has(id)).toBe(true);
      // Simulated scale is ≥ the real browsable count (the headline
      // inflation contract).
      expect(byId.get(id)!.fileCount).toBeGreaterThanOrEqual(count);
    }
  });

  it("is deterministic per (device, generation, fraction)", () => {
    const a = generateResults(usb, 7, 0.9);
    const b = generateResults(usb, 7, 0.9);
    expect(a.nodes.length).toBe(b.nodes.length);
    expect(a.partitions).toEqual(b.partitions);
  });
});

/* ── The store's partition scope ─────────────────────────────────────── */

describe("results store partition scope", () => {
  it("setPartition scopes and clears; null shows everything", () => {
    const store = useResultsStore.getState();
    store.setPartition("p-lost");
    expect(useResultsStore.getState().activePartition).toBe("p-lost");
    store.setPartition(null);
    expect(useResultsStore.getState().activePartition).toBeNull();
  });

  it("partition filter composes with the essential/complete mode", () => {
    // A lost-name node: hidden in essential, visible in complete.
    const node: FileNode = {
      id: 0,
      parentId: null,
      name: "IMG00042.jpg",
      isDir: false,
      category: "images",
      size: 1024,
      modified: Date.now(),
      origin: "lost-name",
      recoverability: "good",
      path: "File Path Lost\\IMG00042.jpg",
      ext: "jpg",
      partition: "p-lost",
    };
    expect(isVisible(node, "essential", { ...EMPTY_FILTER })).toBe(false);
    expect(isVisible(node, "complete", { ...EMPTY_FILTER })).toBe(true);
    // And the junk filter composes independently.
    expect(
      isVisible({ ...node, origin: "deleted", name: "$Recycle.bin" }, "essential", {
        ...EMPTY_FILTER,
        hideJunk: true,
      }),
    ).toBe(false);
  });
});
