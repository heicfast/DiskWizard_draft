/**
 * The mock-core realism contract (browser mock): determinism + tree
 * invariants + the headline reference numbers. When the Rust mock or
 * the production core replaces this module, these invariants ARE the
 * UI-facing contract: progressive fill, category roll-ups, path
 * formatting, the stat scaling.
 */
import { describe, expect, it } from "vitest";
import { DEVICES, GB, makeRng, SDR_DEVICE } from "../data";
import { generateResults } from "../files";
import type { StorageDevice } from "../../types";

const USB_F = DEVICES.externalDevices.find((d) => d.id === "usb-F") as StorageDevice;

describe("generateResults — determinism", () => {
  it("produces byte-identical snapshots for the same device + generation", () => {
    const a = generateResults(USB_F, 1);
    const b = generateResults(USB_F, 1);
    expect(a).toEqual(b);
  });

  it("varies by generation (a rescan presents a fresh tree)", () => {
    const a = generateResults(USB_F, 1);
    const b = generateResults(USB_F, 2);
    expect(a.nodes).not.toEqual(b.nodes);
  });
});

describe("generateResults — progressive fill", () => {
  it("scales the visible tree with the scan fraction", () => {
    const half = generateResults(USB_F, 3, 0.5);
    const full = generateResults(USB_F, 3, 1);
    const halfFiles = half.nodes.filter((n) => !n.isDir).length;
    const fullFiles = full.nodes.filter((n) => !n.isDir).length;
    expect(halfFiles).toBeGreaterThan(0);
    expect(halfFiles).toBeLessThan(fullFiles);
    expect(halfFiles / fullFiles).toBeGreaterThan(0.3);
    expect(halfFiles / fullFiles).toBeLessThan(0.7);
  });

  it("scales the headline stats with the same fraction", () => {
    const full = generateResults(USB_F, 3, 1);
    const half = generateResults(USB_F, 3, 0.5);
    const sum = (s: typeof full) =>
      s.categories.reduce((acc, c) => acc + c.count, 0);
    expect(sum(half)).toBeLessThan(sum(full));
  });
});

describe("generateResults — tree invariants", () => {
  const snap = generateResults(USB_F, 4, 1);

  it("emits compact sequential ids covering the node list", () => {
    expect(snap.nodes[0].id).toBe(0);
    expect(snap.nodes.map((n) => n.id)).toEqual(snap.nodes.map((_, i) => i));
  });

  it("gives every node a resolvable parent chain landing at the root", () => {
    const byId = new Map(snap.nodes.map((n) => [n.id, n]));
    for (const n of snap.nodes) {
      let cur = n;
      let hops = 0;
      while (cur.parentId !== null) {
        cur = byId.get(cur.parentId) as (typeof snap.nodes)[number];
        hops += 1;
        if (hops > 64) throw new Error("parent chain too deep / cyclic");
      }
    }
  });

  it("rolls dir sizes up from their children", () => {
    const childSum = new Map<number, number>();
    for (const n of snap.nodes) {
      if (n.parentId === null) continue;
      childSum.set(n.parentId, (childSum.get(n.parentId) ?? 0) + n.size);
    }
    for (const n of snap.nodes) {
      if (!n.isDir) continue;
      expect(n.size).toBe(childSum.get(n.id) ?? 0);
    }
  });

  it("formats file paths under the device root with backslashes", () => {
    const root = snap.nodes[0]; // synthesized root dir first
    const probe = snap.nodes.find((n) => !n.isDir) as (typeof snap.nodes)[number];
    expect(probe.path.startsWith(root.name)).toBe(true);
    expect(probe.path.includes("\\")).toBe(true);
    expect(probe.name).toBe(`${probe.name}`);
    expect(probe.ext.length).toBeGreaterThan(0);
  });

  it("never grades a carved lost file as excellent (RAW carve = no FS metadata)", () => {
    for (const n of snap.nodes) {
      if (n.isDir) continue;
      if (n.origin === "lost-path" || n.origin === "lost-name") {
        expect(["good", "poor"]).toContain(n.recoverability);
      }
    }
  });

  it("produces files across a healthy spread of categories", () => {
    const cats = new Set(snap.nodes.filter((n) => !n.isDir).map((n) => n.category));
    expect(cats.size).toBeGreaterThanOrEqual(5);
  });

  it("carries the generation + target name through", () => {
    expect(snap.generation).toBe(4);
    expect(snap.targetName).toBe(USB_F.name);
  });
});

describe("generateResults — the RAW SD-card profile", () => {
  it("uses the SDR profile (root label + exFAT recover story)", () => {
    const snap = generateResults(SDR_DEVICE, 5);
    expect(snap.targetName).toBe(SDR_DEVICE.name);
    expect(snap.nodes.length).toBeGreaterThan(0);
  });
});

describe("makeRng — the seeded PRNG", () => {
  it("reproduces sequences exactly", () => {
    const a = makeRng(42);
    const b = makeRng(42);
    const seqA = [a(), a(), a(), a(), a()];
    const seqB = [b(), b(), b(), b(), b()];
    expect(seqA).toEqual(seqB);
  });
  it("stays in [0,1) across long runs", () => {
    const r = makeRng(7);
    for (let i = 0; i < 10_000; i++) {
      const v = r();
      if (v < 0 || v >= 1) throw new Error(`out of range: ${v}`);
    }
  });
  it("differs per seed", () => {
    const a = makeRng(1)();
    const b = makeRng(2)();
    expect(a).not.toBe(b);
  });
});

describe("device inventory realism", () => {
  it("matches the reference capacities (120.5 GB / 232.3 GB system volume)", () => {
    const c = DEVICES.localDisks[0];
    expect(c.used / GB).toBeCloseTo(120.5, 5);
    expect(c.total / GB).toBeCloseTo(232.3, 5);
    expect(c.isSystem).toBe(true);
  });
  it("keeps used <= total for every device", () => {
    for (const d of [...DEVICES.localDisks, ...DEVICES.externalDevices]) {
      expect(d.used).toBeLessThanOrEqual(d.total);
      expect(d.id.length).toBeGreaterThan(0);
    }
  });
});
