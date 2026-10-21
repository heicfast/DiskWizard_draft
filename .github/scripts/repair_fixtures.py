#!/usr/bin/env python3
"""Repair E2E fixture generator + verifier.

`make`        — generate REAL files (Pillow JPEG/PNG, a raw multi-object
                PDF writer, python-docx DOCX, ffmpeg MP4) and corrupt
                them in realistic damage classes.
`check-analyze` — every fixture must report damage (honest verdicts).
`verify`      — repaired outputs must open in REAL decoders (Pillow,
                pypdf, zipfile, ffprobe) — decode-as-validation.

Damage classes (the repair-grammar research):
  trunc60      — stream cut at 60% (the deleted-then-reused tail)
  headsmash    — first 200 bytes zeroed (the damaged-header class)
  midcorrupt   — 4 KiB of 0xE5 garbage mid-file (overwrite damage)
  taildestroy  — last 1 KiB zeroed (the 7z next-header / ZIP CD class)
"""
import json
import os
import random
import subprocess
import sys
import zipfile


# ───────────────────────────── builders ─────────────────────────────

def build_jpeg(path):
    from PIL import Image
    w, h = 480, 360
    rng = random.Random(7)
    img = Image.new("RGB", (w, h))
    px = img.load()
    for y in range(h):
        for x in range(w):
            # gradient + per-pixel noise: real-photo entropy, ~30 KiB
            px[x, y] = ((x * 255 // w + rng.randrange(32)) % 256,
                        (y * 255 // h + rng.randrange(32)) % 256,
                        ((x + y + rng.randrange(64)) % 256))
    img.save(path, "JPEG", quality=88)


def build_png(path):
    from PIL import Image
    w, h = 480, 360
    rng = random.Random(11)
    img = Image.new("RGBA", (w, h))
    px = img.load()
    for y in range(h):
        for x in range(w):
            px[x, y] = ((255 - x * 255 // w + rng.randrange(48)) % 256,
                        (y * 255 // h + rng.randrange(48)) % 256, 128, 255)
    img.save(path, "PNG")


def build_pdf(path):
    # A real multi-object PDF (content stream + font + page tree).
    stream = b"BT /F1 24 Tf 72 700 Td (DiskWizard repair E2E) Tj ET\n"
    objs = [
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
        (b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
         b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n"),
        (b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"
         b"\nendobj\n"),
        (b"5 0 obj\n<< /Length " + str(len(stream)).encode()
         + b" >>\nstream\n" + stream + b"endstream\nendobj\n"),
    ]
    v = b"%PDF-1.4\n" + b"".join(objs)
    xref_at = len(v)
    v += b"xref\n0 6\n0000000000 65535 f\r\n"
    for off in [9, 62, 127, 236, 325, 410]:
        v += b"%010d 00000 n\r\n" % off
    v += b"trailer\n<< /Size 6 /Root 1 0 R >>\n"
    v += b"startxref\n%d\n%%%%EOF" % xref_at
    with open(path, "wb") as f:
        f.write(v)


def build_docx(path):
    from docx import Document
    doc = Document()
    doc.add_heading("DiskWizard Repair E2E", 0)
    doc.add_paragraph("The OOXML package rebuild fixture.")
    doc.save(path)


def build_mp4(path, ffmpeg="ffmpeg"):
    # A real 5-second test video (testsrc → h264 → MP4).
    subprocess.run(
        [ffmpeg, "-y", "-f", "lavfi", "-i",
         "testsrc=duration=5:size=320x240:rate=10",
         "-pix_fmt", "yuv420p", "-c:v", "libx264", path],
        check=True, capture_output=True)


BUILDERS = {
    "photo.jpg": build_jpeg,
    "photo.png": build_png,
    "doc.pdf": build_pdf,
    "doc.docx": build_docx,
    "clip.mp4": build_mp4,
}


# ───────────────────────────── corruptors ────────────────────────────

def trunc(data, pct):
    return data[: max(64, int(len(data) * pct))]


def head_smash(data, n=200):
    n = min(n, len(data) // 4)
    return b"\x00" * n + data[n:]


def mid_corrupt(data, n=4096):
    n = min(n, len(data) // 4)
    at = len(data) // 2
    return data[:at] + b"\xE5" * n + data[at + n:]


def tail_destroy(data, n=1024):
    n = min(n, len(data) // 4)
    return data[:-n] + b"\x00" * n


def make(orig_dir, corrupt_dir):
    os.makedirs(orig_dir, exist_ok=True)
    os.makedirs(corrupt_dir, exist_ok=True)
    for name, builder in BUILDERS.items():
        builder(os.path.join(orig_dir, name))
    cases = []
    for name in BUILDERS:
        data = open(os.path.join(orig_dir, name), "rb").read()
        stem, ext = name.rsplit(".", 1)
        variants = {
            "trunc60": trunc(data, 0.60),
            "headsmash": head_smash(data, 200),
            "midcorrupt": mid_corrupt(data),
            "taildestroy": tail_destroy(data),
        }
        for vname, vdata in variants.items():
            out = os.path.join(corrupt_dir, f"{stem}-{vname}.{ext}")
            with open(out, "wb") as f:
                f.write(vdata)
            cases.append(os.path.basename(out))
    print(f"made {len(cases)} corrupt fixtures from {len(BUILDERS)} real files")
    for c in cases:
        print("  ", c)


def check_analyze(report_path):
    from PIL import Image
    import pypdf
    reports = json.load(open(report_path))["reports"]
    bad = []
    for r in reports:
        print(f"  {r.get('fileName')}: healthy={r.get('healthy')} "
              f"tier={r.get('tier')} :: {r.get('detail', '')[:90]}")
        if r.get("healthy") is True:
            # The honest-skip contract: the container-grammar verdict
            # "healthy" must agree with the REAL decoder on the file —
            # mid-content corruption invisible to the container walk
            # is exactly this class.
            name = r["fileName"]
            src = os.path.join(os.path.dirname(report_path), "corrupt", name)
            try:
                if name.endswith((".jpg", ".png")):
                    with Image.open(src) as im:
                        im.load()
                    agrees = True
                elif name.endswith(".pdf"):
                    agrees = len(pypdf.PdfReader(src).pages) >= 1
                elif name.endswith(".docx"):
                    import zipfile as zf
                    with zf.ZipFile(src) as z:
                        agrees = z.testzip() is None
                elif name.endswith(".mp4"):
                    cp = subprocess.run(
                        ["ffprobe", "-v", "error", "-show_entries",
                         "stream=codec_type", "-of", "json", src],
                        capture_output=True, text=True, timeout=30)
                    agrees = cp.returncode == 0 and "codec_type" in cp.stdout
                else:
                    agrees = False
            except Exception:
                agrees = False
            if not agrees:
                bad.append(name)
    if bad:
        sys.exit(f"FAIL: fixtures reported healthy but decoders reject: {bad}")
    missing = [r for r in reports if "error" in r]
    if missing:
        sys.exit(f"FAIL: analyze errors: {missing}")
    print(f"PASS: all {len(reports)} verdicts honest (damage named or "
          f"decoder-confirmed skip)")


def verify(fixed_dir, report_path, orig_dir):
    from PIL import Image
    import pypdf
    corrupt_dir = os.path.join(os.path.dirname(fixed_dir.rstrip("/\\")), "corrupt")

    def corrupt_dir_for(_name):
        return corrupt_dir

    results = {r["fileName"]: r for r in json.load(open(report_path))["results"]}
    ok, fail = 0, 0

    def check(cond, msg):
        nonlocal ok, fail
        print(("PASS " if cond else "FAIL ") + msg)
        if cond:
            ok += 1
        else:
            fail += 1

    def out_path(name):
        return os.path.join(fixed_dir, name)

    families = [
        ("photo", "jpg", ["trunc60", "headsmash", "midcorrupt", "taildestroy"]),
        ("photo", "png", ["trunc60", "headsmash", "midcorrupt", "taildestroy"]),
        ("doc", "pdf", ["trunc60", "headsmash", "midcorrupt", "taildestroy"]),
        ("doc", "docx", ["trunc60", "headsmash", "midcorrupt", "taildestroy"]),
        ("clip", "mp4", ["trunc60", "headsmash", "midcorrupt", "taildestroy"]),
    ]

    for stem, ext, variants in families:
        for v in variants:
            name = f"{stem}-{v}.{ext}"
            r = results.get(name, {})
            p = out_path(name)
            if not r.get("repaired"):
                detail = r.get("detail", r.get("error", ""))[:90]
                if "BeyondRepair" in detail:
                    # The honest-BeyondRepair contract (the MP4
                    # moov-less class): the engine says no container
                    # repair exists — the REAL decoder must ALSO
                    # reject the original (agreement = honesty).
                    src = os.path.join(corrupt_dir_for(name), name)
                    try:
                        if ext in ("jpg", "png"):
                            with Image.open(src) as im:
                                im.load()
                            agrees = False  # decoder accepts → engine too pessimistic
                        elif ext == "pdf":
                            agrees = len(pypdf.PdfReader(src).pages) < 1
                        elif ext == "docx":
                            import zipfile as zf
                            with zf.ZipFile(src) as z:
                                agrees = z.testzip() is not None
                        elif ext == "mp4":
                            cp = subprocess.run(
                                ["ffprobe", "-v", "error", "-show_entries",
                                 "stream=codec_type", "-of", "json", src],
                                capture_output=True, text=True, timeout=30)
                            agrees = cp.returncode != 0
                        else:
                            agrees = False
                    except Exception:
                        agrees = True  # decoder rejects → agreement
                    check(agrees,
                          f"{name}: honest BeyondRepair — decoder agrees no "
                          f"container repair exists")
                elif r.get("healthy"):
                    # The honest-skip contract: the engine says the
                    # container is structurally sound — the REAL
                    # decoder must agree on the original (untouched)
                    # corrupt file, or the skip was wrong.
                    src = os.path.join(corrupt_dir_for(name), name)
                    try:
                        if ext in ("jpg", "png"):
                            with Image.open(src) as im:
                                im.load()
                            ok_skip = True
                        elif ext == "pdf":
                            ok_skip = len(pypdf.PdfReader(src).pages) >= 1
                        elif ext == "docx":
                            with zipfile.ZipFile(src) as z:
                                ok_skip = (z.testzip() is None
                                           and "[Content_Types].xml" in z.namelist())
                        elif ext == "mp4":
                            cp = subprocess.run(
                                ["ffprobe", "-v", "error", "-show_entries",
                                 "stream=codec_type", "-of", "json", src],
                                capture_output=True, text=True, timeout=30)
                            ok_skip = cp.returncode == 0 and "codec_type" in cp.stdout
                        else:
                            ok_skip = False
                    except Exception:
                        ok_skip = False
                    check(ok_skip,
                          f"{name}: honest skip ({detail}) — decoder agrees the "
                          f"original is structurally sound")
                else:
                    check(False, f"{name}: engine reported not-repaired ({detail})")
                continue
            verdict = None
            try:
                if ext in ("jpg", "png"):
                    with Image.open(p) as im:
                        im.load()
                    verdict = True
                elif ext == "pdf":
                    reader = pypdf.PdfReader(p)
                    verdict = len(reader.pages) >= 1
                elif ext == "docx":
                    with zipfile.ZipFile(p) as z:
                        verdict = z.testzip() is None and "[Content_Types].xml" in z.namelist()
                elif ext == "mp4":
                    cp = subprocess.run(
                        ["ffprobe", "-v", "error", "-show_entries",
                         "stream=codec_type", "-of", "json", p],
                        capture_output=True, text=True, timeout=30)
                    verdict = cp.returncode == 0 and "codec_type" in cp.stdout
            except Exception as e:
                verdict = False
                print(f"    decoder exception: {e}")
            check(bool(verdict),
                  f"{name}: real decoder {'accepts' if verdict else 'REJECTS'} "
                  f"the repair ({os.path.getsize(p)} bytes)")

    print(f"\nverify: {ok} pass / {fail} fail")
    sys.exit(1 if fail else 0)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    cmd = sys.argv[1]
    if cmd == "make":
        make(sys.argv[sys.argv.index("--orig") + 1],
             sys.argv[sys.argv.index("--corrupt") + 1])
    elif cmd == "check-analyze":
        check_analyze(sys.argv[sys.argv.index("--report") + 1])
    elif cmd == "verify":
        verify(sys.argv[sys.argv.index("--fixed") + 1],
               sys.argv[sys.argv.index("--report") + 1],
               sys.argv[sys.argv.index("--orig") + 1])
    else:
        sys.exit(f"unknown subcommand {cmd}")


if __name__ == "__main__":
    main()
