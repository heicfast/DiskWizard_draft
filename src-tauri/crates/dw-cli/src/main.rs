//! # dw-cli — the headless core junction.
//!
//! The owner's session-8 requirement: "figure out someway to test the
//! real core without ui/ux on windows… a cli junction with brief
//! logging and max verbose logs and results all saved and recorded".
//!
//! Every command drives the SAME production engine the Tauri app runs
//! (`dw-core` + `dw-win`) — no simulation, no shortcuts — with:
//! * structured progress on stderr (scan phase / percent / counters),
//! * a full timestamped log (`--log FILE`, everything, always),
//! * machine-readable JSON outputs (`--out FILE`) for CI assertions,
//! * exit codes a workflow can gate on.
//!
//! ```text
//! dw-cli devices [--json]
//! dw-cli smart <device-id> [--json]
//! dw-cli scan <target> [--out results.json] [--log scan.log] [-v]
//!            [--max-seconds N]
//! dw-cli recover <results.json> --dest DIR [--ids 1,2,3]
//!                [--origin deleted|existing|lost-path|lost-name|all]
//!                [--all] [--log recover.log] [-v]
//! dw-cli selftest [-v]            (cross-platform: fixtures → scan →
//!                                  recover → byte-verify)
//! ```
//!
//! `<target>` = a device id (`disk0`, `disk0-part2`, `vol-C`,
//! `disk1-lost2048`) on Windows, or a disk-image file path on any
//! host (`.vhd`/`.img`/`.dd` raw images).

mod logging;
mod recover;
mod repair;
mod scan;
mod selftest;
mod target;

use std::process::ExitCode;

const USAGE: &str = "\
DiskWizard headless core junction — the production engine, no UI.

USAGE:
  dw-cli devices [--json]
  dw-cli smart <device-id> [--json]
  dw-cli scan <target> [--out FILE] [--log FILE] [-v|--verbose]
             [--max-seconds N]
  dw-cli recover <results.json> --dest DIR [--ids 1,2,3 | --all |
                 --origin deleted|existing|lost-path|lost-name|version]
                 [--log FILE] [-v|--verbose]
  dw-cli selftest [-v|--verbose]

TARGETS (scan):
  diskN | diskN-partM | diskN-lostLBA | vol-X   (Windows)
  <path-to-image-file>                            (any host)

OPTIONS:
  --json          Machine-readable output (stdout)
  --out FILE      Write the scan results JSON (nodes + extents)
  --log FILE      Full timestamped log (always verbose in the file)
  -v, --verbose   Mirror the log to stderr
  --max-seconds N Abort the scan after N seconds (CI guard; partial
                  results are kept — same as Stop)
  --dest DIR      Recovery destination root
  --ids LIST      Comma-separated node ids to recover
  --origin FILTER Recover only nodes of this origin
  --all           Recover every file node
  -h, --help      This help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("dw-cli: {e}");
            eprintln!("Run `dw-cli --help` for usage.");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some(first) = args.first() else {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    };
    match first.as_str() {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "devices" => {
            let json = has_flag(args, "--json");
            cmd_devices(json)
        }
        "smart" => {
            let id = args
                .get(1)
                .ok_or_else(|| "smart needs a device id (e.g. disk0)".to_string())?;
            cmd_smart(id)
        }
        "scan" => scan::cmd(args),
        "recover" => recover::cmd(args),
        "repair" => repair::cmd(args),
        "selftest" => selftest::cmd(args),
        other => Err(format!("unknown command `{other}`")),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/* ── devices / smart ─────────────────────────────────────────────── */

fn cmd_devices(json: bool) -> Result<ExitCode, String> {
    #[cfg(windows)]
    {
        let inv = dw_win::devices::enumerate_inventory();
        let payload = serde_json::json!({
            "localDisks": inv.local_disks,
            "externalDevices": inv.external_devices,
        });
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
            );
        } else {
            for d in inv.local_disks.iter().chain(inv.external_devices.iter()) {
                println!(
                    "{:<18} {:<28} {:<8} {:<6} used {}/{}  disk{}",
                    d.id, d.name, d.kind, d.file_system, d.used, d.total, d.disk_index
                );
            }
        }
        Ok(ExitCode::SUCCESS)
    }
    #[cfg(not(windows))]
    {
        let _ = json;
        Err(
            "NOT_AVAILABLE. Device enumeration requires Windows (image files work everywhere)."
                .into(),
        )
    }
}

fn cmd_smart(device_id: &str) -> Result<ExitCode, String> {
    #[cfg(windows)]
    {
        let disk = dw_win::volume::parse_device_id(device_id)
            .map(|t| match t {
                dw_win::volume::DeviceTarget::WholeDisk(n)
                | dw_win::volume::DeviceTarget::Partition { disk: n, .. }
                | dw_win::volume::DeviceTarget::LostPartition { disk: n, .. } => Some(n),
                dw_win::volume::DeviceTarget::Volume(letter) => {
                    dw_win::blockdev::RawHandleDevice::open_volume(letter)
                        .ok()
                        .and_then(|d| d.device_number().ok().map(|(n, _)| n))
                }
            })
            .flatten();
        let Some(index) = disk else {
            return Err(format!("UNKNOWN_DEVICE. {device_id}"));
        };
        let report =
            dw_win::smart::report_for_disk(index).map_err(|e| format!("SMART_UNAVAILABLE. {e}"))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        Ok(ExitCode::SUCCESS)
    }
    #[cfg(not(windows))]
    {
        let _ = device_id;
        Err("NOT_AVAILABLE. SMART requires Windows.".into())
    }
}
