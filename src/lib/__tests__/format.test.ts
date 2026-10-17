/**
 * Display formatting contract tests — every string the UI shapes:
 * bytes, counts, durations, remaining time, dates, temperatures.
 */
import { describe, expect, it } from "vitest";
import {
  clampPercent,
  formatBytes,
  formatCount,
  formatCountFull,
  formatDateTime,
  formatDate,
  formatDuration,
  formatRemaining,
  formatTemp,
} from "../format";

describe("formatBytes", () => {
  it("renders the reference capacity strings exactly", () => {
    // 120.5 GB / 232.3 GB from the cherry-blossom reference capture.
    expect(formatBytes(120.5 * 1024 ** 3)).toBe("120.5 GB");
    expect(formatBytes(232.3 * 1024 ** 3)).toBe("232.3 GB");
  });
  it("renders the headline scan numbers (18.6 GB / 124,532 files)", () => {
    expect(formatBytes(18.6 * 1024 ** 3)).toBe("18.6 GB");
  });
  it("crosses unit boundaries at exact powers of 1024", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 ** 2)).toBe("1.0 MB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
    expect(formatBytes(1024 ** 4)).toBe("1.0 TB");
  });
  it("keeps one decimal under 1000 and rounds above", () => {
    expect(formatBytes(1.5 * 1024)).toBe("1.5 KB");
    expect(formatBytes(999.9 * 1024)).toBe("999.9 KB");
    expect(formatBytes(1500 * 1024 ** 2)).toBe("1.5 GB");
  });
  it("degrades gracefully on invalid input", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe("—");
  });
});

describe("formatCount", () => {
  it("groups small counts with commas", () => {
    expect(formatCount(0)).toBe("0");
    expect(formatCount(9_999)).toBe("9,999");
  });
  it("compacts above 10k (tree badges)", () => {
    expect(formatCount(10_000)).toBe("10.0K");
    expect(formatCount(124_532)).toBe("124.5K");
  });
  it("compacts above a million", () => {
    expect(formatCount(1_139_275)).toBe("1.1M");
  });
  it("degrades gracefully", () => {
    expect(formatCount(-5)).toBe("—");
    expect(formatCount(Number.NaN)).toBe("—");
  });
});

describe("formatCountFull", () => {
  it("renders the reference totals exactly", () => {
    expect(formatCountFull(124532)).toBe("124,532");
    expect(formatCountFull(1139275)).toBe("1,139,275");
  });
});

describe("formatDuration", () => {
  it("renders seconds, minutes and hours forms", () => {
    expect(formatDuration(9)).toBe("9s");
    expect(formatDuration(756)).toBe("12m 36s");
    expect(formatDuration(600)).toBe("10m");
    expect(formatDuration(3840)).toBe("1h 04m");
  });
  it("zero-pads the seconds part", () => {
    expect(formatDuration(601)).toBe("10m 01s");
  });
  it("degrades gracefully", () => {
    expect(formatDuration(-1)).toBe("—");
    expect(formatDuration(Number.NaN)).toBe("—");
  });
});

describe("formatRemaining", () => {
  it("renders the zero-padded research form (07m43s)", () => {
    expect(formatRemaining(463)).toBe("07m43s");
    expect(formatRemaining(0)).toBe("00m00s");
    expect(formatRemaining(59)).toBe("00m59s");
  });
  it("clamps negative estimates to zero", () => {
    expect(formatRemaining(-30)).toBe("00m00s");
  });
});

describe("formatDateTime / formatDate", () => {
  it("renders the file-table format (2023-10-24 14:32)", () => {
    expect(formatDateTime(new Date(2023, 9, 24, 14, 32).getTime() / 1000)).toBe(
      "2023-10-24 14:32",
    );
  });
  it("renders the session-card format (Oct 24, 2023)", () => {
    expect(formatDate(new Date(2023, 9, 24, 14, 32).getTime() / 1000)).toBe("Oct 24, 2023");
  });
  it("maps zero to the em-dash placeholder", () => {
    expect(formatDateTime(0)).toBe("—");
    expect(formatDate(0)).toBe("—");
  });
});

describe("formatTemp / clampPercent", () => {
  it("rounds celsius readings", () => {
    expect(formatTemp(38.4)).toBe("38°C");
    expect(formatTemp(46.6)).toBe("47°C");
  });
  it("clamps progress values into 0..100", () => {
    expect(clampPercent(-3)).toBe(0);
    expect(clampPercent(64.2)).toBe(64.2);
    expect(clampPercent(120)).toBe(100);
  });
});
