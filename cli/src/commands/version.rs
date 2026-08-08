//! `flint version` — detailed build provenance.

/// Render multi-line build provenance for `flint version`.
///
/// Pulled out as a pure function so we can unit-test the layout without
/// shelling out. All inputs come from `env!()` at compile time — fields
/// are constants from the caller's perspective and the function is here
/// purely to centralize formatting.
pub(crate) fn render_version_info() -> String {
    format!(
        "flint {version}\n  Build:       {build}\n  Target:      {target}\n  Fleet sync:  {sync_commit} ({sync_date})\n",
        version = env!("CARGO_PKG_VERSION"),
        build = env!("BUILD_TIMESTAMP"),
        target = env!("TARGET_TRIPLE"),
        sync_commit = env!("FLEET_SYNC_COMMIT"),
        sync_date = env!("FLEET_SYNC_DATE"),
    )
}

pub(crate) fn run() {
    print!("{}", render_version_info());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_info_renders_all_required_fields() {
        // Triage messages from users include this output verbatim, so the
        // four labeled fields (Build / Target / Fleet sync) must always
        // be present. The exact values come from build.rs env injection.
        let out = render_version_info();
        assert!(out.starts_with("flint "), "leading line must be 'flint <version>'");
        assert!(out.contains("Build:"));
        assert!(out.contains("Target:"));
        assert!(out.contains("Fleet sync:"));
        // Trailing newline so shells can append cleanly.
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn version_info_includes_pkg_version() {
        let out = render_version_info();
        assert!(
            out.contains(env!("CARGO_PKG_VERSION")),
            "version line must include CARGO_PKG_VERSION"
        );
    }
}
