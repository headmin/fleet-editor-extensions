//! Installer metadata acquisition — shells out to system tools to read
//! identifier/version/hash from .pkg/.deb/.ipa/.msi/.rpm installers.
//!
//! The parsing half lives in [`crate::pkg`] (`parse_pkg_metadata`,
//! stanza/policy generators); this module owns *acquisition*: spawning
//! `shasum`/`xar`/`ar`/`tar`/`unzip`/`plutil`/`msiinfo`/`rpm` and shepherding
//! their output into a [`PkgInfo`]. Split off from the CLI so every flint
//! face shares one implementation alongside the type it produces.

use crate::pkg::PkgInfo;
use std::path::Path;

/// Everything read from one installer: parsed metadata, the on-disk file
/// name, and the SHA-256 Fleet needs for `hash_sha256:`.
#[derive(Debug, Clone)]
pub struct InstallerInfo {
    pub info: PkgInfo,
    pub filename: String,
    pub sha256: String,
}

/// Inspect any supported installer (.pkg/.deb/.ipa/.msi/.rpm/.exe/.tar.gz).
/// Identifier/version are read where the format and available tools allow;
/// otherwise placeholders are used (the hash is always computed).
#[expect(
    clippy::print_stderr,
    reason = "advisory notes about missing external tools; should become InstallerInfo.notes for the caller to render"
)]
pub fn inspect(path: &Path) -> anyhow::Result<InstallerInfo> {
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("installer")
        .to_string();
    let lower = filename.to_lowercase();

    // .pkg keeps its dedicated xar-based extractor.
    if lower.ends_with(".pkg") {
        return inspect_pkg(path);
    }

    let sha256 = sha256_of(path)?;
    let info = if lower.ends_with(".deb") {
        deb_info(path)
    } else if lower.ends_with(".ipa") {
        ipa_info(path)
    } else if lower.ends_with(".msi") {
        msi_info(path)
    } else if lower.ends_with(".rpm") {
        rpm_info(path)
    } else if lower.ends_with(".exe") || lower.ends_with(".tar.gz") {
        eprintln!(
            "note: {} carries no embedded identifier/version — using placeholders",
            if lower.ends_with(".exe") { ".exe" } else { ".tar.gz" }
        );
        PkgInfo::default()
    } else {
        anyhow::bail!(
            "unsupported installer format: {} (expected .pkg/.deb/.ipa/.msi/.rpm/.exe/.tar.gz)",
            filename
        );
    };
    Ok(InstallerInfo {
        info,
        filename,
        sha256,
    })
}

/// Inspect one `.pkg` via `shasum`/`xar` (macOS product or component archive).
#[expect(
    clippy::print_stderr,
    reason = "advisory notes about missing external tools; should become InstallerInfo.notes for the caller to render"
)]
pub fn inspect_pkg(path: &Path) -> anyhow::Result<InstallerInfo> {
    use crate::pkg::parse_pkg_metadata;
    use std::process::Command;

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package.pkg")
        .to_string();

    let sha256 = sha256_of(path)?;

    // Extract the metadata XML (Distribution for product archives, else the
    // component PackageInfo) into a scratch dir, read it, then clean up.
    let tmp = std::env::temp_dir().join(format!("flint-pkg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;

    let xml = (|| -> Option<String> {
        Command::new("xar")
            .arg("-xf")
            .arg(path)
            .arg("Distribution")
            .arg("-C")
            .arg(&tmp)
            .output()
            .ok()?;
        if let Ok(s) = std::fs::read_to_string(tmp.join("Distribution")) {
            return Some(s);
        }
        // Component package — locate and extract a PackageInfo member.
        let toc = Command::new("xar").arg("-tf").arg(path).output().ok()?;
        let listing = String::from_utf8_lossy(&toc.stdout);
        let member = listing.lines().find(|l| l.ends_with("PackageInfo"))?;
        Command::new("xar")
            .arg("-xf")
            .arg(path)
            .arg(member)
            .arg("-C")
            .arg(&tmp)
            .output()
            .ok()?;
        std::fs::read_to_string(tmp.join(member)).ok()
    })();

    let info = parse_pkg_metadata(xml.as_deref().unwrap_or(""));
    let _ = std::fs::remove_dir_all(&tmp);

    if xml.is_none() {
        eprintln!(
            "warning: could not read package metadata (is `xar` on PATH? macOS only) — \
             identifier/version will be placeholders"
        );
    }

    Ok(InstallerInfo {
        info,
        filename,
        sha256,
    })
}

/// SHA-256 of a file via `shasum`.
fn sha256_of(path: &Path) -> anyhow::Result<String> {
    use std::process::Command;
    let out = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run shasum: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("shasum failed for {}", path.display());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string())
}

/// Run a command, returning trimmed stdout on success (None otherwise).
fn run_ok(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

/// `.deb`: read Package/Version from the control archive via `ar` + `tar`.
#[expect(
    clippy::print_stderr,
    reason = "advisory notes about missing external tools; should become InstallerInfo.notes for the caller to render"
)]
pub(crate) fn deb_info(path: &Path) -> PkgInfo {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut info = PkgInfo::default();
    let members = match run_ok("ar", &["t", &abs.to_string_lossy()]) {
        Some(m) => m,
        None => {
            eprintln!("note: `ar` failed on {} — using placeholders", path.display());
            return info;
        }
    };
    let member = members
        .lines()
        .find(|l| l.trim().starts_with("control.tar"));
    let member = match member {
        Some(m) => m.trim().to_string(),
        None => return info,
    };
    // Extract the control archive, then read ./control out of it.
    let tmp = std::env::temp_dir().join(format!("flint-deb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::create_dir_all(&tmp);
    let archive = tmp.join(&member);
    if let Some(bytes) = std::process::Command::new("ar")
        .arg("p")
        .arg(&abs)
        .arg(&member)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| o.stdout)
    {
        let _ = std::fs::write(&archive, bytes);
        if let Some(control) = run_ok("tar", &["-xOf", &archive.to_string_lossy(), "./control"])
            .or_else(|| run_ok("tar", &["-xOf", &archive.to_string_lossy(), "control"]))
        {
            for line in control.lines() {
                if let Some(v) = line.strip_prefix("Package:") {
                    info.identifier = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("Version:") {
                    info.version = Some(v.trim().to_string());
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    info
}

/// `.ipa`: read CFBundleIdentifier/Version from the app's Info.plist via
/// `unzip` + `plutil`.
pub(crate) fn ipa_info(path: &Path) -> PkgInfo {
    let mut info = PkgInfo::default();
    let p = path.to_string_lossy().to_string();
    let listing = match run_ok("unzip", &["-Z1", &p]) {
        Some(l) => l,
        None => return info,
    };
    // Top-level app Info.plist: Payload/<App>.app/Info.plist (one path segment).
    let member = listing.lines().find(|l| {
        let l = l.trim();
        l.starts_with("Payload/")
            && l.ends_with(".app/Info.plist")
            && l.matches('/').count() == 2
    });
    let member = match member {
        Some(m) => m.trim().to_string(),
        None => return info,
    };
    let tmp = std::env::temp_dir().join(format!("flint-ipa-{}.plist", std::process::id()));
    if let Some(out) = std::process::Command::new("unzip")
        .args(["-p", &p, &member])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| o.stdout)
    {
        let _ = std::fs::write(&tmp, out);
        let t = tmp.to_string_lossy().to_string();
        info.identifier = run_ok("plutil", &["-extract", "CFBundleIdentifier", "raw", "-o", "-", &t])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        info.version =
            run_ok("plutil", &["-extract", "CFBundleShortVersionString", "raw", "-o", "-", &t])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        let _ = std::fs::remove_file(&tmp);
    }
    info
}

/// `.msi`: read ProductName/ProductVersion via `msiinfo` (from msitools) if
/// installed; otherwise placeholders.
#[expect(
    clippy::print_stderr,
    reason = "advisory notes about missing external tools; should become InstallerInfo.notes for the caller to render"
)]
pub(crate) fn msi_info(path: &Path) -> PkgInfo {
    let mut info = PkgInfo::default();
    match run_ok("msiinfo", &["export", &path.to_string_lossy(), "Property"]) {
        Some(table) => {
            for line in table.lines() {
                let mut cols = line.splitn(2, '\t');
                match (cols.next(), cols.next()) {
                    (Some("ProductName"), Some(v)) => info.identifier = Some(v.trim().to_string()),
                    (Some("ProductVersion"), Some(v)) => info.version = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }
        None => eprintln!("note: `msiinfo` (msitools) not found — .msi metadata is placeholder"),
    }
    info
}

/// `.rpm`: read NAME/VERSION via `rpm` if installed; otherwise placeholders.
#[expect(
    clippy::print_stderr,
    reason = "advisory notes about missing external tools; should become InstallerInfo.notes for the caller to render"
)]
pub(crate) fn rpm_info(path: &Path) -> PkgInfo {
    let mut info = PkgInfo::default();
    match run_ok(
        "rpm",
        &["-qp", "--queryformat", "%{NAME}\t%{VERSION}", &path.to_string_lossy()],
    ) {
        Some(out) => {
            let mut cols = out.trim().splitn(2, '\t');
            info.identifier = cols.next().map(|s| s.to_string()).filter(|s| !s.is_empty());
            info.version = cols.next().map(|s| s.to_string()).filter(|s| !s.is_empty());
        }
        None => eprintln!("note: `rpm` not found — .rpm metadata is placeholder"),
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn inspect_rejects_missing_file() {
        let err = inspect(Path::new("/nonexistent/thing.pkg")).unwrap_err();
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn inspect_rejects_unsupported_extension() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("app.dmg");
        fs::write(&f, "x").unwrap();
        let err = inspect(&f).unwrap_err();
        assert!(err.to_string().contains("unsupported installer format"));
    }

}
