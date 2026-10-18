//! Mock-core results generator — the Rust twin of `src/mock/files.ts`:
//! a deterministic found-file tree per (device, generation, fraction),
//! with the category mix, recoverability grading, the RAW-carve
//! branches (File Path Lost / Tags Extracted) and per-category count
//! roll-ups reported at the simulated find scale.
use serde_json::{Value, json};

/// Mulberry32 (exact 32-bit translation of the TS mock's PRNG —
/// same algorithm, same [0,1) output range) so results are
/// deterministic per seed and screenshots/CI stay stable.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(1 | t);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0
    }
    fn pick<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        &arr[(self.next() * arr.len() as f64) as usize % arr.len()]
    }
}

const PHOTO_NAMES: [&str; 20] = [
    "Vacation Photo",
    "Family",
    "Sunset Over Lake",
    "Birthday Party",
    "IMG_2023",
    "DSC_0481",
    "wedding-ceremony",
    "Tokyo Street at Night",
    "Cherry Blossoms",
    "Mountain Hike",
    "Screenshot",
    "Dinner with Friends",
    "Graduation",
    "PXL_20231024",
    "New Year Fireworks",
    "Beach Day",
    "team-offsite",
    "Product Prototype",
    "Whiteboard Session",
    "Pet Portrait",
];
const PHOTO_EXT: [&str; 7] = ["jpg", "jpeg", "png", "heic", "webp", "gif", "tiff"];
const DOC_NAMES: [&str; 17] = [
    "Project Proposal",
    "Invoice",
    "Resume",
    "Budget 2026",
    "Meeting Notes",
    "Contract Draft",
    "Thesis Chapter",
    "Tax Return",
    "Lease Agreement",
    "Quarterly Report",
    "Presentation Deck",
    "Recipe Collection",
    "Budget",
    "Insurance Policy",
    "Cover Letter",
    "Business Plan",
    "Whiteboard Sketch",
];
const DOC_EXT: [&str; 8] = ["docx", "pdf", "xlsx", "pptx", "doc", "txt", "md", "csv"];
const VIDEO_NAMES: [&str; 12] = [
    "holiday-4k",
    "birthday-recap",
    "Screen Recording",
    "Gameplay",
    "Drone Flight",
    "DJI_0214",
    "wedding-highlights",
    "concert-clip",
    "timelapse-sunset",
    "MVI_1304",
    "interview-take3",
    "screen-capture-bug",
];
const VIDEO_EXT: [&str; 5] = ["mp4", "mov", "avi", "mkv", "m4v"];
const AUDIO_NAMES: [&str; 8] = [
    "Podcast Episode",
    "Voice Memo",
    "Demo Track",
    "Lecture Recording",
    "mixtape-final",
    "Interview",
    "VoiceNotes",
    "field-recording",
];
const AUDIO_EXT: [&str; 5] = ["mp3", "wav", "m4a", "flac", "aac"];
const ARCHIVE_NAMES: [&str; 8] = [
    "backup-2026",
    "Photos Archive",
    "project-export",
    "old-website",
    "tax-documents",
    "disk-image",
    "downloads-bundle",
    "design-assets",
];
const ARCHIVE_EXT: [&str; 5] = ["zip", "rar", "7z", "tar.gz", "iso"];

/// Email store names (Outlook PST/OST, Outlook Express DBX, raw EML)
/// — enough variety that the browsable list reads like real mail data.
const EMAIL_NAMES: [&str; 12] = [
    "Inbox",
    "Sent Mail",
    "Archive folder",
    "Local Folders",
    "Outlook Data File",
    "Personal Folders",
    "Mailbox - Alex",
    "Old Mail 2019",
    "backup-mail",
    "Deleted Items",
    "work-mailbox",
    "family-mailbox",
];
const EMAIL_EXT: [&str; 6] = ["pst", "pst", "pst", "ost", "dbx", "eml"];

struct Profile {
    rows: usize,
    mix: [f64; 7], // images, videos, audio, documents, archives, email, other
    root: &'static str,
    duration: u64,
}

fn profile(device_id: &str) -> Profile {
    match device_id {
        "usb-F" => Profile {
            rows: 2_560,
            mix: [34.0, 16.0, 9.0, 22.0, 5.0, 1.0, 13.0],
            root: "USB Drive (F:)",
            duration: 756,
        },
        "disk0-part2" => Profile {
            rows: 3_200,
            mix: [18.0, 12.0, 8.0, 26.0, 7.0, 2.0, 27.0],
            root: "Windows (C:)",
            duration: 2_340,
        },
        "disk1-part1" => Profile {
            rows: 2_880,
            mix: [22.0, 24.0, 10.0, 20.0, 8.0, 1.0, 15.0],
            root: "Data (D:)",
            duration: 1_580,
        },
        "disk1-part2" => Profile {
            rows: 2_200,
            mix: [10.0, 6.0, 4.0, 44.0, 12.0, 1.0, 23.0],
            root: "Projects (E:)",
            duration: 720,
        },
        "ssd-G" => Profile {
            rows: 2_400,
            mix: [24.0, 28.0, 8.0, 18.0, 10.0, 1.0, 11.0],
            root: "Portable SSD (G:)",
            duration: 1_920,
        },
        _ => Profile {
            rows: 1_800,
            mix: [26.0, 14.0, 8.0, 24.0, 8.0, 1.0, 19.0],
            root: "Scan",
            duration: 640,
        },
    }
}

/// Recursive directory-node factory: ensures `path` exists in the
/// tree (creating ancestors as needed) and returns its node id.
fn dir_node(
    path: &str,
    root: &str,
    dirs: &mut std::collections::HashMap<String, usize>,
    nodes: &mut Vec<Value>,
    now: u64,
) -> usize {
    if let Some(&id) = dirs.get(path) {
        return id;
    }
    let parent = if path == root {
        None
    } else {
        let parent_path = match path.rfind('\\') {
            Some(0) | None => root.to_string(),
            Some(i) => path[..i].to_string(),
        };
        Some(dir_node(&parent_path, root, dirs, nodes, now) as i64)
    };
    let name = if path == root {
        root.to_string()
    } else {
        path.rsplit('\\').next().unwrap_or(path).to_string()
    };
    let id = nodes.len();
    nodes.push(json!({
        "id": id, "parentId": parent, "name": name, "isDir": true,
        "category": "other", "size": 0, "modified": now,
        "origin": "existing", "recoverability": "excellent",
        "path": path, "ext": "",
    }));
    dirs.insert(path.to_string(), id);
    id
}

fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 2_166_136_261;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

const CATS: [&str; 7] = [
    "images",
    "videos",
    "audio",
    "documents",
    "archives",
    "email",
    "other",
];

fn size_for(cat: usize, rng: &mut Rng) -> u64 {
    let r = rng.next();
    match cat {
        0 => ((0.4 + r * 7.6) * MB) as u64,
        1 => ((24.0 + r * 940.0) * MB) as u64,
        2 => ((2.2 + r * 9.5) * MB) as u64,
        3 => ((0.03 + r * 3.4) * MB) as u64,
        4 => ((3.0 + r * 260.0) * MB) as u64,
        _ => ((0.01 + r * 42.0) * MB) as u64,
    }
}

const MB: f64 = 1024.0 * 1024.0;

/// Generate the snapshot (see the TS twin for the scale contract:
/// `categories` reports the simulated headline numbers; `nodes` are
/// the real browsable rows).
pub fn generate(device_id: &str, generation: u64, fraction: f64) -> Value {
    let prof = profile(device_id);
    let seed = hash_str(device_id) ^ generation.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut rng = Rng(seed as u32);
    let total_rows = ((prof.rows as f64) * fraction.clamp(0.01, 1.0)).round() as usize;
    let mix_sum: f64 = prof.mix.iter().sum();

    let mut nodes: Vec<Value> = Vec::with_capacity(total_rows + 16);
    // Dir registry: path → node id.
    let mut dirs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let root = prof.root.to_string();
    dir_node(&root, &root, &mut dirs, &mut nodes, now);
    let lost = format!("{root}\\File Path Lost");
    dir_node(&lost, &root, &mut dirs, &mut nodes, now);
    let tags = format!("{root}\\Tags Extracted");
    dir_node(&tags, &root, &mut dirs, &mut nodes, now);

    // Per-category real counts (for the simulated-scale roll-up).
    let mut real_counts = [0u64; 7];
    let mut real_sizes = [0u64; 7];

    for i in 0..total_rows.max(1) {
        // Weighted category dice.
        let mut dice = rng.next() * mix_sum;
        let mut cat = 6;
        for (c, w) in prof.mix.iter().enumerate() {
            if dice < *w {
                cat = c;
                break;
            }
            dice -= w;
        }

        // Origin distribution (existing/deleted/lost-path/lost-name/tags/version).
        let od = rng.next();
        let origin = if od < 0.52 {
            "existing"
        } else if od < 0.82 {
            "deleted"
        } else if od < 0.93 {
            "lost-path"
        } else if od < 0.965 {
            "lost-name"
        } else if od < 0.99 {
            "tags"
        } else {
            "version"
        };

        // Name + ext.
        let (base, ext): (String, String) = match cat {
            0 => (
                (*rng.pick(&PHOTO_NAMES)).to_string(),
                (*rng.pick(&PHOTO_EXT)).to_string(),
            ),
            1 => (
                (*rng.pick(&VIDEO_NAMES)).to_string(),
                (*rng.pick(&VIDEO_EXT)).to_string(),
            ),
            2 => (
                (*rng.pick(&AUDIO_NAMES)).to_string(),
                (*rng.pick(&AUDIO_EXT)).to_string(),
            ),
            3 => (
                if rng.next() < 0.4 {
                    format!(
                        "{} {}",
                        rng.pick(&DOC_NAMES),
                        2024 + (rng.next() * 3.0) as u64
                    )
                } else {
                    rng.pick(&DOC_NAMES).to_string()
                },
                (*rng.pick(&DOC_EXT)).to_string(),
            ),
            4 => (
                (*rng.pick(&ARCHIVE_NAMES)).to_string(),
                (*rng.pick(&ARCHIVE_EXT)).to_string(),
            ),
            5 => (
                (*rng.pick(&EMAIL_NAMES)).to_string(),
                (*rng.pick(&EMAIL_EXT)).to_string(),
            ),
            _ => (
                rng.pick(&["settings", "config", "index", "cache", "db"])
                    .to_string(),
                rng.pick(&["dat", "db", "html", "exe"]).to_string(),
            ),
        };
        let name = if origin == "lost-name" {
            format!("{}{:05}", if cat == 0 { "IMG" } else { "FILE" }, 10_000 + i)
        } else if origin == "version" {
            format!("{base} (v2)")
        } else {
            base.clone()
        };
        let ext = if origin == "lost-name" {
            if cat == 0 { "jpg".into() } else { "bin".into() }
        } else {
            ext
        };

        let size = size_for(cat, &mut rng);
        // Date: recent-weighted within ~3 years.
        let days_back = ((rng.next().powf(1.8) * 1_090.0) as u64).min(1_089);
        let modified = now.saturating_sub(days_back * 86400 + (rng.next() * 86400.0) as u64);

        // Grade.
        let g = rng.next();
        let grade = match origin {
            "existing" => {
                if g < 0.82 {
                    "excellent"
                } else if g < 0.97 {
                    "good"
                } else {
                    "poor"
                }
            }
            "deleted" => {
                if g < 0.55 {
                    "excellent"
                } else if g < 0.88 {
                    "good"
                } else {
                    "poor"
                }
            }
            "tags" => {
                if g < 0.6 {
                    "excellent"
                } else {
                    "good"
                }
            }
            _ => {
                if g < 0.85 {
                    "good"
                } else {
                    "poor"
                }
            }
        };

        // Folder (carve branches override).
        let folder = if origin == "lost-path" || origin == "lost-name" {
            lost.clone()
        } else if origin == "tags" {
            tags.clone()
        } else {
            let sub = match cat {
                0 => *rng.pick(&["Photos", "Camera Roll", "Pictures", "Screenshots", "Travel"]),
                1 => *rng.pick(&["Videos", "Movies", "Drone Footage"]),
                2 => *rng.pick(&["Music", "Voice Memos", "Podcasts"]),
                3 => *rng.pick(&["Documents", "Work", "Desktop", "Invoices 2026", "Projects"]),
                4 => *rng.pick(&["Backups", "Downloads", "Archives"]),
                _ => *rng.pick(&["Downloads", "AppData", "Misc"]),
            };
            format!("{root}\\{sub}")
        };
        let dir_id = dir_node(&folder, &root, &mut dirs, &mut nodes, now) as i64;
        let path = format!("{folder}\\{name}.{ext}");

        let dims = if cat == 0 && rng.next() > 0.72 {
            let sets = [
                (4032, 3024),
                (3024, 4032),
                (6000, 4000),
                (1920, 1080),
                (3456, 4608),
                (5472, 3648),
            ];
            let d = *rng.pick(&sets);
            Some(d)
        } else {
            None
        };

        real_counts[cat] += 1;
        real_sizes[cat] += size;

        let mut node = json!({
            "id": nodes.len(), "parentId": dir_id,
            "name": format!("{name}.{ext}"), "isDir": false,
            "category": CATS[cat], "size": size, "modified": modified,
            "origin": origin, "recoverability": grade, "path": path, "ext": ext,
        });
        if let Some((w, h)) = dims {
            node["width"] = json!(w);
            node["height"] = json!(h);
        }
        if origin == "deleted" {
            node["deletedAgo"] = json!((now.saturating_sub(modified)) / 86400);
        }
        nodes.push(node);
    }

    // Roll up dir sizes (children after parents by construction; walk
    // in reverse so leaves settle first).
    let mut sizes: Vec<i128> = nodes
        .iter()
        .map(|n| n["size"].as_u64().unwrap_or(0) as i128)
        .collect();
    for i in (0..nodes.len()).rev() {
        if let Some(p) = nodes[i]["parentId"].as_i64() {
            if p >= 0 {
                sizes[p as usize] += sizes[i];
            }
        }
    }
    for (i, s) in sizes.into_iter().enumerate() {
        if let Some(o) = nodes[i].as_object_mut() {
            o.insert("size".into(), json!(s.max(0) as u64));
        }
    }

    // Simulated-scale categories.
    let scale = if total_rows > 0 {
        let (files, _) = super::data::totals(device_id);
        files as f64 * fraction / total_rows as f64
    } else {
        0.0
    };
    let categories: Vec<Value> = CATS
        .iter()
        .enumerate()
        .map(|(c, cat)| {
            json!({
                "category": cat,
                "count": (real_counts[c] as f64 * scale).round() as u64,
                "size": (real_sizes[c] as f64 * scale).round() as u64,
            })
        })
        .collect();

    json!({
        "generation": generation,
        "targetName": prof.root,
        "categories": categories,
        "nodes": nodes,
        "durationSec": (prof.duration as f64 * fraction).round() as u64,
    })
}
