use std::collections::HashMap;
use once_cell::sync::Lazy;

pub struct OsqueryTable {
    pub name: &'static str,
    pub platforms: Vec<&'static str>,
    pub description: &'static str,
}

/// osquery table compatibility matrix
/// Source: https://osquery.io/schema/
pub static OSQUERY_TABLES: Lazy<HashMap<&'static str, OsqueryTable>> = Lazy::new(|| {
    let mut tables = HashMap::new();

    // macOS-specific tables
    tables.insert("alf", OsqueryTable {
        name: "alf",
        platforms: vec!["darwin"],
        description: "macOS application layer firewall",
    });

    tables.insert("disk_encryption", OsqueryTable {
        name: "disk_encryption",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Disk encryption status",
    });

    tables.insert("filevault_status", OsqueryTable {
        name: "filevault_status",
        platforms: vec!["darwin"],
        description: "macOS FileVault encryption status",
    });

    tables.insert("managed_policies", OsqueryTable {
        name: "managed_policies",
        platforms: vec!["darwin"],
        description: "macOS managed policies",
    });

    tables.insert("authorization_mechanisms", OsqueryTable {
        name: "authorization_mechanisms",
        platforms: vec!["darwin"],
        description: "macOS authorization mechanisms",
    });

    tables.insert("gatekeeper", OsqueryTable {
        name: "gatekeeper",
        platforms: vec!["darwin"],
        description: "macOS Gatekeeper status",
    });

    tables.insert("sip_config", OsqueryTable {
        name: "sip_config",
        platforms: vec!["darwin"],
        description: "macOS System Integrity Protection config",
    });

    // Windows-specific tables
    tables.insert("bitlocker_info", OsqueryTable {
        name: "bitlocker_info",
        platforms: vec!["windows"],
        description: "Windows BitLocker encryption info",
    });

    tables.insert("windows_security_center", OsqueryTable {
        name: "windows_security_center",
        platforms: vec!["windows"],
        description: "Windows Security Center status",
    });

    tables.insert("windows_firewall_rules", OsqueryTable {
        name: "windows_firewall_rules",
        platforms: vec!["windows"],
        description: "Windows firewall rules",
    });

    tables.insert("registry", OsqueryTable {
        name: "registry",
        platforms: vec!["windows"],
        description: "Windows registry",
    });

    tables.insert("windows_update_history", OsqueryTable {
        name: "windows_update_history",
        platforms: vec!["windows"],
        description: "Windows update history",
    });

    // Cross-platform tables
    tables.insert("users", OsqueryTable {
        name: "users",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Local user accounts",
    });

    tables.insert("processes", OsqueryTable {
        name: "processes",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Running processes",
    });

    tables.insert("system_info", OsqueryTable {
        name: "system_info",
        platforms: vec!["darwin", "linux", "windows"],
        description: "System information",
    });

    tables.insert("os_version", OsqueryTable {
        name: "os_version",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Operating system version",
    });

    tables.insert("usb_devices", OsqueryTable {
        name: "usb_devices",
        platforms: vec!["darwin", "linux", "windows"],
        description: "USB devices",
    });

    tables.insert("logged_in_users", OsqueryTable {
        name: "logged_in_users",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Currently logged in users",
    });

    tables.insert("listening_ports", OsqueryTable {
        name: "listening_ports",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Listening network ports",
    });

    tables.insert("interface_addresses", OsqueryTable {
        name: "interface_addresses",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Network interface addresses",
    });

    tables.insert("startup_items", OsqueryTable {
        name: "startup_items",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Startup items/services",
    });

    tables.insert("certificates", OsqueryTable {
        name: "certificates",
        platforms: vec!["darwin", "linux", "windows"],
        description: "System certificates",
    });

    tables.insert("chrome_extensions", OsqueryTable {
        name: "chrome_extensions",
        platforms: vec!["darwin", "linux", "windows", "chrome"],
        description: "Chrome browser extensions",
    });

    tables.insert("installed_applications", OsqueryTable {
        name: "installed_applications",
        platforms: vec!["darwin", "windows"],
        description: "Installed applications",
    });

    tables.insert("programs", OsqueryTable {
        name: "programs",
        platforms: vec!["windows"],
        description: "Installed programs",
    });

    tables.insert("apps", OsqueryTable {
        name: "apps",
        platforms: vec!["darwin"],
        description: "macOS applications",
    });

    // Linux-specific tables
    tables.insert("apt_sources", OsqueryTable {
        name: "apt_sources",
        platforms: vec!["linux"],
        description: "APT package sources",
    });

    tables.insert("deb_packages", OsqueryTable {
        name: "deb_packages",
        platforms: vec!["linux"],
        description: "Debian packages",
    });

    tables.insert("rpm_packages", OsqueryTable {
        name: "rpm_packages",
        platforms: vec!["linux"],
        description: "RPM packages",
    });

    tables.insert("selinux_settings", OsqueryTable {
        name: "selinux_settings",
        platforms: vec!["linux"],
        description: "SELinux settings",
    });

    tables.insert("iptables", OsqueryTable {
        name: "iptables",
        platforms: vec!["linux"],
        description: "iptables firewall rules",
    });

    tables
});
