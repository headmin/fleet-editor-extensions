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

    // =========================================================================
    // Additional cross-platform tables (Security & Monitoring)
    // =========================================================================

    tables.insert("file", OsqueryTable {
        name: "file",
        platforms: vec!["darwin", "linux", "windows"],
        description: "File metadata and attributes",
    });

    tables.insert("hash", OsqueryTable {
        name: "hash",
        platforms: vec!["darwin", "linux", "windows"],
        description: "File hashes (MD5, SHA1, SHA256)",
    });

    tables.insert("yara", OsqueryTable {
        name: "yara",
        platforms: vec!["darwin", "linux", "windows"],
        description: "YARA pattern scanning results",
    });

    tables.insert("crontab", OsqueryTable {
        name: "crontab",
        platforms: vec!["darwin", "linux"],
        description: "Scheduled cron jobs",
    });

    tables.insert("scheduled_tasks", OsqueryTable {
        name: "scheduled_tasks",
        platforms: vec!["windows"],
        description: "Windows scheduled tasks",
    });

    tables.insert("services", OsqueryTable {
        name: "services",
        platforms: vec!["windows"],
        description: "Windows services",
    });

    tables.insert("launchd", OsqueryTable {
        name: "launchd",
        platforms: vec!["darwin"],
        description: "macOS launchd jobs",
    });

    tables.insert("systemd_units", OsqueryTable {
        name: "systemd_units",
        platforms: vec!["linux"],
        description: "systemd service units",
    });

    // =========================================================================
    // Network tables
    // =========================================================================

    tables.insert("routes", OsqueryTable {
        name: "routes",
        platforms: vec!["darwin", "linux", "windows"],
        description: "System routing table",
    });

    tables.insert("arp_cache", OsqueryTable {
        name: "arp_cache",
        platforms: vec!["darwin", "linux", "windows"],
        description: "ARP cache entries",
    });

    tables.insert("dns_resolvers", OsqueryTable {
        name: "dns_resolvers",
        platforms: vec!["darwin", "linux", "windows"],
        description: "DNS resolver settings",
    });

    tables.insert("etc_hosts", OsqueryTable {
        name: "etc_hosts",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Hosts file entries",
    });

    tables.insert("interface_details", OsqueryTable {
        name: "interface_details",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Network interface details",
    });

    tables.insert("socket_events", OsqueryTable {
        name: "socket_events",
        platforms: vec!["darwin", "linux"],
        description: "Socket connection events",
    });

    tables.insert("process_open_sockets", OsqueryTable {
        name: "process_open_sockets",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Open sockets by process",
    });

    tables.insert("connectivity", OsqueryTable {
        name: "connectivity",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Network connectivity status",
    });

    // =========================================================================
    // User & Authentication tables
    // =========================================================================

    tables.insert("groups", OsqueryTable {
        name: "groups",
        platforms: vec!["darwin", "linux", "windows"],
        description: "User groups",
    });

    tables.insert("user_groups", OsqueryTable {
        name: "user_groups",
        platforms: vec!["darwin", "linux", "windows"],
        description: "User group memberships",
    });

    tables.insert("shadow", OsqueryTable {
        name: "shadow",
        platforms: vec!["linux"],
        description: "Shadow password database",
    });

    tables.insert("authorized_keys", OsqueryTable {
        name: "authorized_keys",
        platforms: vec!["darwin", "linux"],
        description: "SSH authorized keys",
    });

    tables.insert("user_ssh_keys", OsqueryTable {
        name: "user_ssh_keys",
        platforms: vec!["darwin", "linux"],
        description: "User SSH keys",
    });

    tables.insert("ssh_configs", OsqueryTable {
        name: "ssh_configs",
        platforms: vec!["darwin", "linux"],
        description: "SSH configuration files",
    });

    tables.insert("last", OsqueryTable {
        name: "last",
        platforms: vec!["darwin", "linux"],
        description: "Last login history",
    });

    tables.insert("sudoers", OsqueryTable {
        name: "sudoers",
        platforms: vec!["darwin", "linux"],
        description: "Sudoers file entries",
    });

    // =========================================================================
    // System information tables
    // =========================================================================

    tables.insert("uptime", OsqueryTable {
        name: "uptime",
        platforms: vec!["darwin", "linux", "windows"],
        description: "System uptime",
    });

    tables.insert("cpu_info", OsqueryTable {
        name: "cpu_info",
        platforms: vec!["darwin", "linux", "windows"],
        description: "CPU information",
    });

    tables.insert("memory_info", OsqueryTable {
        name: "memory_info",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Memory statistics",
    });

    tables.insert("mounts", OsqueryTable {
        name: "mounts",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Mounted filesystems",
    });

    tables.insert("disk_info", OsqueryTable {
        name: "disk_info",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Physical disk information",
    });

    tables.insert("block_devices", OsqueryTable {
        name: "block_devices",
        platforms: vec!["darwin", "linux"],
        description: "Block devices",
    });

    tables.insert("kernel_info", OsqueryTable {
        name: "kernel_info",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Kernel version info",
    });

    tables.insert("kernel_modules", OsqueryTable {
        name: "kernel_modules",
        platforms: vec!["darwin", "linux"],
        description: "Loaded kernel modules",
    });

    tables.insert("pci_devices", OsqueryTable {
        name: "pci_devices",
        platforms: vec!["darwin", "linux", "windows"],
        description: "PCI devices",
    });

    tables.insert("hardware_events", OsqueryTable {
        name: "hardware_events",
        platforms: vec!["darwin", "linux"],
        description: "Hardware events",
    });

    tables.insert("system_controls", OsqueryTable {
        name: "system_controls",
        platforms: vec!["darwin", "linux"],
        description: "System sysctl settings",
    });

    // =========================================================================
    // Process tables
    // =========================================================================

    tables.insert("process_envs", OsqueryTable {
        name: "process_envs",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Process environment variables",
    });

    tables.insert("process_memory_map", OsqueryTable {
        name: "process_memory_map",
        platforms: vec!["darwin", "linux"],
        description: "Process memory mappings",
    });

    tables.insert("process_events", OsqueryTable {
        name: "process_events",
        platforms: vec!["darwin", "linux"],
        description: "Process start/exit events",
    });

    tables.insert("process_file_events", OsqueryTable {
        name: "process_file_events",
        platforms: vec!["darwin", "linux"],
        description: "File events by process",
    });

    // =========================================================================
    // macOS-specific additional tables
    // =========================================================================

    tables.insert("keychain_items", OsqueryTable {
        name: "keychain_items",
        platforms: vec!["darwin"],
        description: "macOS keychain items",
    });

    tables.insert("keychain_acls", OsqueryTable {
        name: "keychain_acls",
        platforms: vec!["darwin"],
        description: "macOS keychain ACLs",
    });

    tables.insert("preferences", OsqueryTable {
        name: "preferences",
        platforms: vec!["darwin"],
        description: "macOS application preferences",
    });

    tables.insert("plist", OsqueryTable {
        name: "plist",
        platforms: vec!["darwin"],
        description: "Property list files",
    });

    tables.insert("nvram", OsqueryTable {
        name: "nvram",
        platforms: vec!["darwin"],
        description: "NVRAM settings",
    });

    tables.insert("xprotect_entries", OsqueryTable {
        name: "xprotect_entries",
        platforms: vec!["darwin"],
        description: "XProtect malware entries",
    });

    tables.insert("xprotect_meta", OsqueryTable {
        name: "xprotect_meta",
        platforms: vec!["darwin"],
        description: "XProtect metadata",
    });

    tables.insert("safari_extensions", OsqueryTable {
        name: "safari_extensions",
        platforms: vec!["darwin"],
        description: "Safari browser extensions",
    });

    tables.insert("time_machine_backups", OsqueryTable {
        name: "time_machine_backups",
        platforms: vec!["darwin"],
        description: "Time Machine backup status",
    });

    tables.insert("time_machine_destinations", OsqueryTable {
        name: "time_machine_destinations",
        platforms: vec!["darwin"],
        description: "Time Machine backup destinations",
    });

    tables.insert("location_services", OsqueryTable {
        name: "location_services",
        platforms: vec!["darwin"],
        description: "Location services status",
    });

    tables.insert("screenlock", OsqueryTable {
        name: "screenlock",
        platforms: vec!["darwin"],
        description: "Screen lock settings",
    });

    tables.insert("sharing_preferences", OsqueryTable {
        name: "sharing_preferences",
        platforms: vec!["darwin"],
        description: "macOS sharing preferences",
    });

    tables.insert("mdm", OsqueryTable {
        name: "mdm",
        platforms: vec!["darwin"],
        description: "MDM enrollment status",
    });

    tables.insert("app_schemes", OsqueryTable {
        name: "app_schemes",
        platforms: vec!["darwin"],
        description: "App URL schemes",
    });

    tables.insert("es_process_events", OsqueryTable {
        name: "es_process_events",
        platforms: vec!["darwin"],
        description: "Endpoint Security process events",
    });

    // =========================================================================
    // Windows-specific additional tables
    // =========================================================================

    tables.insert("patches", OsqueryTable {
        name: "patches",
        platforms: vec!["windows"],
        description: "Windows patches/hotfixes",
    });

    tables.insert("drivers", OsqueryTable {
        name: "drivers",
        platforms: vec!["windows"],
        description: "Windows drivers",
    });

    tables.insert("shared_resources", OsqueryTable {
        name: "shared_resources",
        platforms: vec!["windows"],
        description: "Windows shared resources",
    });

    tables.insert("wmi_cli_event_consumers", OsqueryTable {
        name: "wmi_cli_event_consumers",
        platforms: vec!["windows"],
        description: "WMI command-line event consumers",
    });

    tables.insert("wmi_event_filters", OsqueryTable {
        name: "wmi_event_filters",
        platforms: vec!["windows"],
        description: "WMI event filters",
    });

    tables.insert("wmi_filter_consumer_binding", OsqueryTable {
        name: "wmi_filter_consumer_binding",
        platforms: vec!["windows"],
        description: "WMI filter-consumer bindings",
    });

    tables.insert("wmi_script_event_consumers", OsqueryTable {
        name: "wmi_script_event_consumers",
        platforms: vec!["windows"],
        description: "WMI script event consumers",
    });

    tables.insert("windows_events", OsqueryTable {
        name: "windows_events",
        platforms: vec!["windows"],
        description: "Windows event log entries",
    });

    tables.insert("windows_security_products", OsqueryTable {
        name: "windows_security_products",
        platforms: vec!["windows"],
        description: "Windows security products",
    });

    tables.insert("ntfs_acl_permissions", OsqueryTable {
        name: "ntfs_acl_permissions",
        platforms: vec!["windows"],
        description: "NTFS ACL permissions",
    });

    tables.insert("autoexec", OsqueryTable {
        name: "autoexec",
        platforms: vec!["windows"],
        description: "Autoexec.bat entries",
    });

    tables.insert("appcompat_shims", OsqueryTable {
        name: "appcompat_shims",
        platforms: vec!["windows"],
        description: "Application compatibility shims",
    });

    tables.insert("ie_extensions", OsqueryTable {
        name: "ie_extensions",
        platforms: vec!["windows"],
        description: "Internet Explorer extensions",
    });

    tables.insert("powershell_events", OsqueryTable {
        name: "powershell_events",
        platforms: vec!["windows"],
        description: "PowerShell script events",
    });

    // =========================================================================
    // Browser tables
    // =========================================================================

    tables.insert("firefox_addons", OsqueryTable {
        name: "firefox_addons",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Firefox browser addons",
    });

    // =========================================================================
    // Additional Security tables
    // =========================================================================

    tables.insert("augeas", OsqueryTable {
        name: "augeas",
        platforms: vec!["darwin", "linux"],
        description: "Configuration file parsing via Augeas",
    });

    tables.insert("carves", OsqueryTable {
        name: "carves",
        platforms: vec!["darwin", "linux", "windows"],
        description: "File carving status",
    });

    tables.insert("curl", OsqueryTable {
        name: "curl",
        platforms: vec!["darwin", "linux", "windows"],
        description: "HTTP request results",
    });

    tables.insert("curl_certificate", OsqueryTable {
        name: "curl_certificate",
        platforms: vec!["darwin", "linux", "windows"],
        description: "TLS certificate information",
    });

    // =========================================================================
    // ChromeOS tables
    // =========================================================================

    tables.insert("chrome_extension_content_scripts", OsqueryTable {
        name: "chrome_extension_content_scripts",
        platforms: vec!["darwin", "linux", "windows", "chrome"],
        description: "Chrome extension content scripts",
    });

    // =========================================================================
    // Package manager tables
    // =========================================================================

    tables.insert("homebrew_packages", OsqueryTable {
        name: "homebrew_packages",
        platforms: vec!["darwin"],
        description: "Homebrew packages",
    });

    tables.insert("npm_packages", OsqueryTable {
        name: "npm_packages",
        platforms: vec!["darwin", "linux", "windows"],
        description: "npm packages",
    });

    tables.insert("python_packages", OsqueryTable {
        name: "python_packages",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Python packages",
    });

    tables.insert("atom_packages", OsqueryTable {
        name: "atom_packages",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Atom editor packages",
    });

    tables.insert("chocolatey_packages", OsqueryTable {
        name: "chocolatey_packages",
        platforms: vec!["windows"],
        description: "Chocolatey packages",
    });

    tables.insert("portage_packages", OsqueryTable {
        name: "portage_packages",
        platforms: vec!["linux"],
        description: "Gentoo portage packages",
    });

    // =========================================================================
    // Virtualization tables
    // =========================================================================

    tables.insert("docker_containers", OsqueryTable {
        name: "docker_containers",
        platforms: vec!["darwin", "linux"],
        description: "Docker containers",
    });

    tables.insert("docker_images", OsqueryTable {
        name: "docker_images",
        platforms: vec!["darwin", "linux"],
        description: "Docker images",
    });

    tables.insert("docker_info", OsqueryTable {
        name: "docker_info",
        platforms: vec!["darwin", "linux"],
        description: "Docker system info",
    });

    tables.insert("docker_networks", OsqueryTable {
        name: "docker_networks",
        platforms: vec!["darwin", "linux"],
        description: "Docker networks",
    });

    tables.insert("docker_volumes", OsqueryTable {
        name: "docker_volumes",
        platforms: vec!["darwin", "linux"],
        description: "Docker volumes",
    });

    tables.insert("docker_container_mounts", OsqueryTable {
        name: "docker_container_mounts",
        platforms: vec!["darwin", "linux"],
        description: "Docker container mount points",
    });

    tables.insert("docker_container_ports", OsqueryTable {
        name: "docker_container_ports",
        platforms: vec!["darwin", "linux"],
        description: "Docker container port mappings",
    });

    tables.insert("docker_container_processes", OsqueryTable {
        name: "docker_container_processes",
        platforms: vec!["darwin", "linux"],
        description: "Processes in Docker containers",
    });

    tables.insert("docker_container_labels", OsqueryTable {
        name: "docker_container_labels",
        platforms: vec!["darwin", "linux"],
        description: "Docker container labels",
    });

    // =========================================================================
    // Additional utility tables
    // =========================================================================

    tables.insert("time", OsqueryTable {
        name: "time",
        platforms: vec!["darwin", "linux", "windows"],
        description: "Current system time",
    });

    tables.insert("osquery_info", OsqueryTable {
        name: "osquery_info",
        platforms: vec!["darwin", "linux", "windows"],
        description: "osquery version info",
    });

    tables.insert("osquery_flags", OsqueryTable {
        name: "osquery_flags",
        platforms: vec!["darwin", "linux", "windows"],
        description: "osquery runtime flags",
    });

    tables.insert("osquery_extensions", OsqueryTable {
        name: "osquery_extensions",
        platforms: vec!["darwin", "linux", "windows"],
        description: "osquery extensions",
    });

    tables.insert("osquery_schedule", OsqueryTable {
        name: "osquery_schedule",
        platforms: vec!["darwin", "linux", "windows"],
        description: "osquery schedule status",
    });

    tables.insert("osquery_packs", OsqueryTable {
        name: "osquery_packs",
        platforms: vec!["darwin", "linux", "windows"],
        description: "osquery query packs",
    });

    tables.insert("osquery_registry", OsqueryTable {
        name: "osquery_registry",
        platforms: vec!["darwin", "linux", "windows"],
        description: "osquery registry plugins",
    });

    tables.insert("osquery_events", OsqueryTable {
        name: "osquery_events",
        platforms: vec!["darwin", "linux", "windows"],
        description: "osquery event publishers",
    });

    tables
});
