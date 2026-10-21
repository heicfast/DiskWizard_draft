import { describe, expect, it } from "vitest";
import { scanActive, scanPhaseLabel } from "../scan";
import type { ScanProgress } from "../../types";

const phases = [
  ["idle", "Ready to scan", false],
  ["preparing", "Preparing scan", true],
  ["quick", "Quick scan", true],
  ["deep", "Deep scan", true],
  ["organizing", "Organizing recovered files", true],
  ["paused", "Scan paused", true],
  ["done", "Scan complete", false],
  ["stopped", "Scan stopped", false],
  ["error", "Scan needs attention", false],
] as const satisfies ReadonlyArray<readonly [ScanProgress["phase"], string, boolean]>;

function progress(phase: ScanProgress["phase"]): ScanProgress {
  return {
    generation: 1,
    targetId: "disk0-part1",
    targetName: "Windows (C:)",
    phase,
    percent: 12,
    stats: { filesFound: 4, bytesFound: 512, sectorsRead: 1, sectorsTotal: 8 },
    elapsedSec: 1,
    remainingSec: 2,
    searchingLabel: "Searching: C:\\",
  };
}

describe("scan phase presentation", () => {
  it.each(phases)("uses accurate copy and active state for %s", (phase, label, active) => {
    expect(scanPhaseLabel(phase)).toBe(label);
    expect(scanActive(progress(phase))).toBe(active);
  });

  it("treats a missing progress payload as inactive", () => {
    expect(scanActive(null)).toBe(false);
  });
});
