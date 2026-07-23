// Build script: on Windows, embed the multi-resolution `assets/icons/icon.ico`
// into the executable's resource section so the Windows taskbar (and
// pinned shortcuts) show the proper logo from the moment the app launches.
//
// Bevy 0.18's `Window` struct doesn't expose an icon, and the runtime
// `winit::Icon::from_rgba` only updates the in-window icon *after* the OS
// has cached the executable's resource-section icon. Without this build
// step the taskbar shows a generic Windows icon even though the splash
// window's title bar / Alt-Tab shows the correct logo.
//
// Skipped on non-Windows targets because `winresource` is Windows-only
// (the Cargo.toml puts it under [target.'cfg(windows)'.build-dependencies]).

fn main() {
    #[cfg(windows)]
    {
        let ico_path = "assets/icons/icon.ico";
        if std::path::Path::new(ico_path).exists() {
            let mut res = winresource::WindowsResource::new();
            // Set the application icon (RT_ICON group, ID 1).
            // winresource wires this into a generated .rc that the
            // MSVC linker consumes via embed-resource.
            res.set_icon(ico_path);
            res.compile().expect("failed to embed Windows icon resource");
            println!("cargo:rerun-if-changed={}", ico_path);
        } else {
            // Don't fail the build if the icon hasn't been generated
            // yet — `python scripts/build_icons.py` is a separate
            // manual step in the asset pipeline. Warn so the missing
            // asset is noticed at build time.
            println!(
                "cargo:warning=windows taskbar icon skipped: {} not found; \
                 run `python scripts/build_icons.py` to generate it",
                ico_path
            );
        }
    }

    // No-op on non-Windows targets (the cfg block above is the entire body).
}
