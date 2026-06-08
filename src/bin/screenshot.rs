//! Headless screenshot driver.
//!
//! Loads a RON manifest of `(slot, menu, submenu_path, wait_frames)`
//! entries, queues each as a `QueuedCapture`, and exits when the queue
//! drains. On Linux, run under Xvfb in CI:
//!
//! ```text
//! xvfb-run -a -s "-screen 0 1920x1080x24" \
//!   cargo run --release --bin screenshot -- \
//!   --manifest tools/screenshot_manifest_v1.ron \
//!   --out docs/UI/baselines/v1
//! ```
//!
//! The `--out` flag overrides the manifest's `out_dir` so a single
//! manifest can be re-run into a different baseline label (e.g. v1 → v2
//! once the harmonization PRs land).

use std::path::PathBuf;

use bevy::prelude::*;
use helios_ascension::app::build_helios_app;
use helios_ascension::game_state::GameMenu;
use helios_ascension::ui::screenshot::{
    parse_menu, PendingScreenshotAction, QueuedCapture, ScreenshotManifest,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (manifest_path, out_override) = parse_args(&args);

    let manifest_text = std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| {
        eprintln!(
            "[screenshot] failed to read manifest {}: {e}",
            manifest_path.display()
        );
        std::process::exit(2);
    });
    let manifest: ScreenshotManifest = ron::from_str(&manifest_text).unwrap_or_else(|e| {
        eprintln!("[screenshot] failed to parse manifest: {e}");
        std::process::exit(2);
    });

    let out_dir = out_override
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&manifest.out_dir));
    std::fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        eprintln!(
            "[screenshot] failed to create out dir {}: {e}",
            out_dir.display()
        );
        std::process::exit(2);
    });

    eprintln!(
        "[screenshot] manifest v{} — {} entries → {}",
        manifest.version,
        manifest.entries.len(),
        out_dir.display()
    );

    let mut app = build_helios_app();
    {
        let mut pending = app.world_mut().resource_mut::<PendingScreenshotAction>();
        pending.exit_when_drained = true;
        // The last screenshot's observer may take a few extra frames to
        // fire on a slow CI GPU; 30 frames (0.5 s at 60 fps) is plenty.
        pending.post_drain_frames = 30;
        for entry in manifest.entries {
            let Some(menu) = parse_menu(&entry.menu) else {
                eprintln!(
                    "[screenshot] unknown menu '{}' in entry '{}' — aborting",
                    entry.menu, entry.name
                );
                std::process::exit(2);
            };
            if !entry.submenu_path.is_empty() {
                eprintln!(
                    "[screenshot] entry '{}' carries submenu_path {:?}; \
                     v1 manifest does not yet drive submenus. Reserved for GRA-60.",
                    entry.name, entry.submenu_path
                );
            }
            let out_path = out_dir.join(format!("{}.png", entry.name));
            pending.enqueue(QueuedCapture {
                slot_name: entry.name,
                menu: Some(menu),
                out_path,
                wait_frames: entry.wait_frames,
                frames_remaining: entry.wait_frames,
            });
        }
    }

    // Make sure the menu we drive starts from a known-good state.
    {
        let mut active = app
            .world_mut()
            .resource_mut::<helios_ascension::game_state::ActiveMenu>();
        active.current = GameMenu::Main;
    }

    // Reference GameMenu here so the import is kept even if the manifest
    // does not yet use the Survey variant. This keeps the import list
    // honest until GRA-60 expands it.
    let _ = GameMenu::Survey;

    app.run();
}

fn parse_args(args: &[String]) -> (PathBuf, Option<String>) {
    let mut manifest: Option<PathBuf> = None;
    let mut out: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" | "-m" => {
                manifest = args.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--out" | "-o" => {
                out = args.get(i + 1).cloned();
                i += 2;
            }
            "--help" | "-h" => {
                eprintln!(
                    "usage: screenshot --manifest <path.ron> [--out <dir>]\n\n\
                     Loads a ScreenshotManifest, captures every entry to\n\
                     <out>/<name>.png, and exits. --out overrides the\n\
                     manifest's out_dir when supplied."
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("[screenshot] unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    let manifest = manifest.unwrap_or_else(|| {
        eprintln!(
            "[screenshot] --manifest <path> is required\n\n\
             Try --help for usage."
        );
        std::process::exit(2);
    });
    (manifest, out)
}
