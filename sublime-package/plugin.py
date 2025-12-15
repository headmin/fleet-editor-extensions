"""
LSP-fleet: Sublime Text LSP client for Fleet GitOps YAML validation.

This plugin provides Language Server Protocol support for Fleet GitOps
configuration files, with automatic binary download from GitHub releases.
"""

import os
import platform
import shutil
import stat
import subprocess
import tarfile
import urllib.request
import json
from pathlib import Path

import sublime

# LSP imports
from LSP.plugin import AbstractPlugin, ClientConfig, WorkspaceFolder
from LSP.plugin.core.typing import Optional, List, Dict, Any

BINARY_NAME = "fleet-schema-gen"
GITHUB_REPO = "fleetdm/fleet"
PACKAGE_NAME = "LSP-fleet"


def get_platform_suffix() -> Optional[str]:
    """Get the platform-specific suffix for the binary download."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "darwin":
        if machine in ("arm64", "aarch64"):
            return "darwin-arm64"
        elif machine in ("x86_64", "amd64"):
            return "darwin-x64"
    elif system == "linux":
        if machine in ("x86_64", "amd64"):
            return "linux-x64"
        elif machine in ("aarch64", "arm64"):
            return "linux-arm64"
    elif system == "windows":
        return "windows-x64"

    return None


def get_storage_path() -> Path:
    """Get the package storage path for downloaded binaries."""
    cache_path = sublime.cache_path()
    storage_path = Path(cache_path) / "Package Storage" / PACKAGE_NAME
    storage_path.mkdir(parents=True, exist_ok=True)
    return storage_path


def find_binary_in_path() -> Optional[str]:
    """Try to find the binary in PATH."""
    return shutil.which(BINARY_NAME)


def find_binary_in_common_paths() -> Optional[str]:
    """Try to find the binary in common installation locations."""
    home = os.path.expanduser("~")
    common_paths = [
        os.path.join(home, ".cargo", "bin", BINARY_NAME),
        "/opt/homebrew/bin/" + BINARY_NAME,
        "/usr/local/bin/" + BINARY_NAME,
        "/usr/bin/" + BINARY_NAME,
    ]

    for path in common_paths:
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path

    return None


def get_latest_release_info() -> Optional[Dict[str, Any]]:
    """Fetch latest release info from GitHub."""
    url = f"https://api.github.com/repos/{GITHUB_REPO}/releases"

    try:
        req = urllib.request.Request(url)
        req.add_header("Accept", "application/vnd.github.v3+json")
        req.add_header("User-Agent", f"LSP-fleet/{PACKAGE_NAME}")

        with urllib.request.urlopen(req, timeout=30) as response:
            releases = json.loads(response.read().decode("utf-8"))

            # Find first release with fleet-schema-gen assets
            for release in releases:
                for asset in release.get("assets", []):
                    if asset["name"].startswith(BINARY_NAME):
                        return release

    except Exception as e:
        print(f"[{PACKAGE_NAME}] Failed to fetch release info: {e}")

    return None


def download_binary() -> Optional[str]:
    """Download the binary from GitHub releases."""
    platform_suffix = get_platform_suffix()
    if not platform_suffix:
        print(f"[{PACKAGE_NAME}] Unsupported platform: {platform.system()} {platform.machine()}")
        return None

    storage_path = get_storage_path()
    binary_path = storage_path / BINARY_NAME

    # Check for updates periodically
    release = get_latest_release_info()
    if not release:
        # Fall back to existing binary if we can't fetch release
        if binary_path.exists():
            return str(binary_path)
        print(f"[{PACKAGE_NAME}] Could not fetch release info from GitHub")
        return None

    version = release["tag_name"].lstrip("v")
    asset_name = f"{BINARY_NAME}-{version}-{platform_suffix}.tar.gz"

    # Find matching asset
    asset_url = None
    for asset in release.get("assets", []):
        if asset["name"] == asset_name:
            asset_url = asset["browser_download_url"]
            break

    if not asset_url:
        print(f"[{PACKAGE_NAME}] No matching asset found: {asset_name}")
        return None

    # Check if we already have this version
    version_file = storage_path / "version"
    if binary_path.exists() and version_file.exists():
        current_version = version_file.read_text().strip()
        if current_version == version:
            return str(binary_path)

    print(f"[{PACKAGE_NAME}] Downloading {asset_name}...")

    try:
        archive_path = storage_path / asset_name

        # Download archive
        req = urllib.request.Request(asset_url)
        req.add_header("User-Agent", f"LSP-fleet/{PACKAGE_NAME}")

        with urllib.request.urlopen(req, timeout=120) as response:
            with open(archive_path, "wb") as f:
                f.write(response.read())

        # Extract binary
        with tarfile.open(archive_path, "r:gz") as tar:
            for member in tar.getmembers():
                if member.name == BINARY_NAME or member.name.endswith("/" + BINARY_NAME):
                    member.name = BINARY_NAME
                    tar.extract(member, storage_path)
                    break

        # Make executable
        binary_path.chmod(binary_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        # Clean up archive
        archive_path.unlink()

        # Save version
        version_file.write_text(version)

        print(f"[{PACKAGE_NAME}] Successfully installed {BINARY_NAME} {version}")
        return str(binary_path)

    except Exception as e:
        print(f"[{PACKAGE_NAME}] Failed to download binary: {e}")
        # Fall back to existing binary
        if binary_path.exists():
            return str(binary_path)
        return None


def get_binary_path() -> Optional[str]:
    """Get the binary path, trying multiple methods."""
    # 1. Check user-configured path
    settings = sublime.load_settings("LSP-fleet.sublime-settings")
    custom_path = settings.get("binary_path")
    if custom_path and os.path.isfile(custom_path):
        return custom_path

    # 2. Check PATH
    path = find_binary_in_path()
    if path:
        return path

    # 3. Check common locations
    path = find_binary_in_common_paths()
    if path:
        return path

    # 4. Download from GitHub
    path = download_binary()
    if path:
        return path

    return None


class FleetLsp(AbstractPlugin):
    """Fleet GitOps LSP client for Sublime Text."""

    @classmethod
    def name(cls) -> str:
        return PACKAGE_NAME

    @classmethod
    def basedir(cls) -> str:
        return str(get_storage_path())

    @classmethod
    def server_version(cls) -> str:
        """Return server version string."""
        binary = get_binary_path()
        if binary:
            try:
                result = subprocess.run(
                    [binary, "--version"],
                    capture_output=True,
                    text=True,
                    timeout=5
                )
                if result.returncode == 0:
                    return result.stdout.strip().split()[-1]
            except Exception:
                pass
        return "unknown"

    @classmethod
    def needs_update_or_installation(cls) -> bool:
        """Check if the server needs installation."""
        return get_binary_path() is None

    @classmethod
    def install_or_update(cls) -> None:
        """Install or update the server."""
        path = download_binary()
        if not path:
            raise RuntimeError("Failed to download fleet-schema-gen binary")

    @classmethod
    def configuration(cls) -> sublime.Settings:
        """Return plugin configuration."""
        return sublime.load_settings("LSP-fleet.sublime-settings")

    @classmethod
    def additional_variables(cls) -> Dict[str, str]:
        """Return additional variables for command substitution."""
        binary = get_binary_path()
        return {
            "binary_path": binary or "",
        }

    def on_pre_server_command(
        self,
        command: List[str],
        done_callback: callable
    ) -> bool:
        """Called before starting the server."""
        return False  # Let the default behavior proceed


def plugin_loaded() -> None:
    """Called when the plugin is loaded."""
    print(f"[{PACKAGE_NAME}] Plugin loaded")

    # Trigger initial download in background
    def check_installation():
        path = get_binary_path()
        if path:
            print(f"[{PACKAGE_NAME}] Using binary: {path}")
        else:
            print(f"[{PACKAGE_NAME}] Binary not found - install will be attempted on first use")

    sublime.set_timeout_async(check_installation, 1000)


def plugin_unloaded() -> None:
    """Called when the plugin is unloaded."""
    pass
