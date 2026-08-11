//! osquery table compatibility matrix.
//!
//! Used by `PlatformCompatibilityRule` to detect queries that reference
//! tables unavailable on the declared platform.
//!
//! AUTO-GENERATED from the osquery upstream schema. Do not hand-edit.
//! Regenerate via `python3 scripts/sync-osquery-schema.py`.
//! Schema version: 5.23.1
//! Source: https://github.com/osquery/osquery-site/tree/main/src/data/osquery_schema_versions

use once_cell::sync::Lazy;
use std::collections::HashMap;

pub struct OsqueryTable {
    pub name: &'static str,
    pub platforms: Vec<&'static str>,
    pub description: &'static str,
}

/// 290 tables (osquery 5.23.1 + Fleet overlay).
pub static OSQUERY_TABLES: Lazy<HashMap<&'static str, OsqueryTable>> = Lazy::new(|| {
    let mut tables = HashMap::new();

    tables.insert(
        "account_policy_data",
        OsqueryTable {
            name: "account_policy_data",
            platforms: vec!["darwin"],
            description: "Additional macOS user account data from the AccountPolicy section of OpenDirectory.",
        },
    );
    tables.insert(
        "acpi_tables",
        OsqueryTable {
            name: "acpi_tables",
            platforms: vec!["darwin", "linux"],
            description: "Firmware ACPI functional table common metadata and content.",
        },
    );
    tables.insert(
        "ad_config",
        OsqueryTable {
            name: "ad_config",
            platforms: vec!["darwin"],
            description: "macOS Active Directory configuration.",
        },
    );
    tables.insert(
        "alf",
        OsqueryTable {
            name: "alf",
            platforms: vec!["darwin"],
            description: "macOS application layer firewall (ALF) service details.",
        },
    );
    tables.insert(
        "alf_exceptions",
        OsqueryTable {
            name: "alf_exceptions",
            platforms: vec!["darwin"],
            description: "macOS application layer firewall (ALF) service exceptions.",
        },
    );
    tables.insert(
        "alf_explicit_auths",
        OsqueryTable {
            name: "alf_explicit_auths",
            platforms: vec!["darwin"],
            description: "ALF services explicitly allowed to perform networking. Not supported on macOS 15+ (returns no results).",
        },
    );
    tables.insert(
        "app_schemes",
        OsqueryTable {
            name: "app_schemes",
            platforms: vec!["darwin"],
            description: "macOS application schemes and handlers (e.g., http, file, mailto).",
        },
    );
    tables.insert(
        "apparmor_events",
        OsqueryTable {
            name: "apparmor_events",
            platforms: vec!["linux"],
            description: "Track AppArmor events.",
        },
    );
    tables.insert(
        "apparmor_profiles",
        OsqueryTable {
            name: "apparmor_profiles",
            platforms: vec!["linux"],
            description: "Track active AppArmor profiles.",
        },
    );
    tables.insert(
        "appcompat_shims",
        OsqueryTable {
            name: "appcompat_shims",
            platforms: vec!["windows"],
            description: "Application Compatibility shims are a way to persist malware. This table presents the AppCompat Shim information from the registry in a nice format. See http://files.brucon.org/2015/Tomczak_and_Ballenthin_Shims_for_the_Win.pdf for more details.",
        },
    );
    tables.insert(
        "apps",
        OsqueryTable {
            name: "apps",
            platforms: vec!["darwin"],
            description: "macOS applications installed in known search paths (e.g., /Applications).",
        },
    );
    tables.insert(
        "apt_sources",
        OsqueryTable {
            name: "apt_sources",
            platforms: vec!["linux"],
            description: "Current list of APT repositories or software channels.",
        },
    );
    tables.insert(
        "arp_cache",
        OsqueryTable {
            name: "arp_cache",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Address resolution cache, both static and dynamic (from ARP, NDP).",
        },
    );
    tables.insert(
        "asl",
        OsqueryTable {
            name: "asl",
            platforms: vec!["darwin"],
            description: "Queries the Apple System Log data structure for system events.",
        },
    );
    tables.insert(
        "atom_packages",
        OsqueryTable {
            name: "atom_packages",
            platforms: vec!["darwin", "linux"],
            description: "Atom editor packages installed in a user's home directory.",
        },
    );
    tables.insert(
        "augeas",
        OsqueryTable {
            name: "augeas",
            platforms: vec!["darwin", "linux"],
            description: "Configuration files parsed by augeas.",
        },
    );
    tables.insert(
        "authenticode",
        OsqueryTable {
            name: "authenticode",
            platforms: vec!["windows"],
            description: "File (executable, bundle, installer, disk) code signing status.",
        },
    );
    tables.insert(
        "authorization_mechanisms",
        OsqueryTable {
            name: "authorization_mechanisms",
            platforms: vec!["darwin"],
            description: "macOS Authorization mechanisms database.",
        },
    );
    tables.insert(
        "authorizations",
        OsqueryTable {
            name: "authorizations",
            platforms: vec!["darwin"],
            description: "macOS Authorization rights database.",
        },
    );
    tables.insert(
        "authorized_keys",
        OsqueryTable {
            name: "authorized_keys",
            platforms: vec!["darwin", "linux"],
            description: "A line-delimited authorized_keys table.",
        },
    );
    tables.insert(
        "autoexec",
        OsqueryTable {
            name: "autoexec",
            platforms: vec!["windows"],
            description: "Aggregate of executables that will automatically execute on the target machine. This is an amalgamation of other tables like services, scheduled_tasks, startup_items and more.",
        },
    );
    tables.insert(
        "azure_instance_metadata",
        OsqueryTable {
            name: "azure_instance_metadata",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Azure instance metadata.",
        },
    );
    tables.insert(
        "azure_instance_tags",
        OsqueryTable {
            name: "azure_instance_tags",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Azure instance tags.",
        },
    );
    tables.insert(
        "background_activities_moderator",
        OsqueryTable {
            name: "background_activities_moderator",
            platforms: vec!["windows"],
            description: "Background Activities Moderator (BAM) tracks application execution.",
        },
    );
    tables.insert(
        "battery",
        OsqueryTable {
            name: "battery",
            platforms: vec!["darwin", "windows"],
            description: "Provides information about the internal battery of a laptop. Note: On Windows, columns with Ah or mAh units assume that the battery is 12V.",
        },
    );
    tables.insert(
        "bitlocker_info",
        OsqueryTable {
            name: "bitlocker_info",
            platforms: vec!["windows"],
            description: "Retrieve bitlocker status of the machine.",
        },
    );
    tables.insert(
        "block_devices",
        OsqueryTable {
            name: "block_devices",
            platforms: vec!["darwin", "linux"],
            description: "Block (buffered access) device file nodes: disks, ramdisks, and DMG containers.",
        },
    );
    tables.insert(
        "bpf_process_events",
        OsqueryTable {
            name: "bpf_process_events",
            platforms: vec!["linux"],
            description: "Track time/action process executions.",
        },
    );
    tables.insert(
        "bpf_socket_events",
        OsqueryTable {
            name: "bpf_socket_events",
            platforms: vec!["linux"],
            description: "Track network socket opens and closes.",
        },
    );
    tables.insert(
        "browser_plugins",
        OsqueryTable {
            name: "browser_plugins",
            platforms: vec!["darwin"],
            description: "All C/NPAPI browser plugin details for all users. C/NPAPI has been deprecated on all major browsers. To query for plugins on modern browsers, try: `chrome_extensions` `firefox_addons` `safari_extensions`.",
        },
    );
    tables.insert(
        "carbon_black_info",
        OsqueryTable {
            name: "carbon_black_info",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Returns info about a Carbon Black sensor install.",
        },
    );
    tables.insert(
        "carves",
        OsqueryTable {
            name: "carves",
            platforms: vec!["darwin", "linux", "windows"],
            description: "List the set of completed and in-progress carves. If carve=1 then the query is treated as a new carve request.",
        },
    );
    tables.insert(
        "certificate_trust_settings",
        OsqueryTable {
            name: "certificate_trust_settings",
            platforms: vec!["darwin"],
            description: "Certificate Authorities trust settings installed in Keychains/ca-bundles.",
        },
    );
    tables.insert(
        "certificates",
        OsqueryTable {
            name: "certificates",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Certificate Authorities installed in Keychains/ca-bundles. NOTE: osquery limits frequent access to keychain files on macOS. This limit is controlled by keychain_access_interval flag. On macOS, 'path' may point to either a keychain file or a DER/PEM-encoded certificate file; non-keychain files are parsed as DER/PEM.",
        },
    );
    tables.insert(
        "chassis_info",
        OsqueryTable {
            name: "chassis_info",
            platforms: vec!["windows"],
            description: "Display information pertaining to the chassis and its security status.",
        },
    );
    tables.insert(
        "chocolatey_packages",
        OsqueryTable {
            name: "chocolatey_packages",
            platforms: vec!["windows"],
            description: "Chocolatey packages installed in a system.",
        },
    );
    tables.insert(
        "chrome_extension_content_scripts",
        OsqueryTable {
            name: "chrome_extension_content_scripts",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Chrome browser extension content scripts.",
        },
    );
    tables.insert(
        "chrome_extensions",
        OsqueryTable {
            name: "chrome_extensions",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Chrome-based browser extensions.",
        },
    );
    tables.insert(
        "connected_displays",
        OsqueryTable {
            name: "connected_displays",
            platforms: vec!["darwin"],
            description: "Provides information about the connected displays of the machine.",
        },
    );
    tables.insert(
        "connectivity",
        OsqueryTable {
            name: "connectivity",
            platforms: vec!["windows"],
            description: "Provides the overall system's network state.",
        },
    );
    tables.insert(
        "cpu_info",
        OsqueryTable {
            name: "cpu_info",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Retrieve cpu hardware info of the machine.",
        },
    );
    tables.insert(
        "cpu_time",
        OsqueryTable {
            name: "cpu_time",
            platforms: vec!["darwin", "linux"],
            description: "Displays information from /proc/stat file about the time the cpu cores spent in different parts of the system.",
        },
    );
    tables.insert(
        "cpuid",
        OsqueryTable {
            name: "cpuid",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Useful CPU features from the cpuid ASM call.",
        },
    );
    tables.insert(
        "crashes",
        OsqueryTable {
            name: "crashes",
            platforms: vec!["darwin"],
            description: "Application, System, and Mobile App crash logs.",
        },
    );
    tables.insert(
        "crontab",
        OsqueryTable {
            name: "crontab",
            platforms: vec!["darwin", "linux"],
            description: "Line parsed values from system and user cron/tab.",
        },
    );
    tables.insert(
        "cups_destinations",
        OsqueryTable {
            name: "cups_destinations",
            platforms: vec!["darwin"],
            description: "Returns all configured printers.",
        },
    );
    tables.insert(
        "cups_jobs",
        OsqueryTable {
            name: "cups_jobs",
            platforms: vec!["darwin"],
            description: "Returns all completed print jobs from cups.",
        },
    );
    tables.insert(
        "curl",
        OsqueryTable {
            name: "curl",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Perform an http request and return stats about it.",
        },
    );
    tables.insert(
        "curl_certificate",
        OsqueryTable {
            name: "curl_certificate",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Inspect TLS certificates by connecting to input hostnames.",
        },
    );
    tables.insert(
        "deb_package_files",
        OsqueryTable {
            name: "deb_package_files",
            platforms: vec!["linux"],
            description: "Installed files from DEB packages that are currently installed on the system.",
        },
    );
    tables.insert(
        "deb_packages",
        OsqueryTable {
            name: "deb_packages",
            platforms: vec!["linux"],
            description: "The installed DEB package database.",
        },
    );
    tables.insert(
        "default_environment",
        OsqueryTable {
            name: "default_environment",
            platforms: vec!["windows"],
            description: "Default environment variables and values.",
        },
    );
    tables.insert(
        "device_file",
        OsqueryTable {
            name: "device_file",
            platforms: vec!["darwin", "linux"],
            description: "Similar to the file table, but use TSK and allow block address access.",
        },
    );
    tables.insert(
        "device_firmware",
        OsqueryTable {
            name: "device_firmware",
            platforms: vec!["darwin"],
            description: "A best-effort list of discovered firmware versions.",
        },
    );
    tables.insert(
        "device_hash",
        OsqueryTable {
            name: "device_hash",
            platforms: vec!["darwin", "linux"],
            description: "Similar to the hash table, but use TSK and allow block address access.",
        },
    );
    tables.insert(
        "device_partitions",
        OsqueryTable {
            name: "device_partitions",
            platforms: vec!["darwin", "linux"],
            description: "Use TSK to enumerate details about partitions on a disk device.",
        },
    );
    tables.insert(
        "deviceguard_status",
        OsqueryTable {
            name: "deviceguard_status",
            platforms: vec!["windows"],
            description: "Retrieve DeviceGuard info of the machine.",
        },
    );
    tables.insert(
        "disk_encryption",
        OsqueryTable {
            name: "disk_encryption",
            platforms: vec!["darwin", "linux"],
            description: "Disk encryption status and information.",
        },
    );
    tables.insert(
        "disk_events",
        OsqueryTable {
            name: "disk_events",
            platforms: vec!["darwin"],
            description: "Track DMG disk image events (appearance/disappearance) when opened.",
        },
    );
    tables.insert(
        "disk_info",
        OsqueryTable {
            name: "disk_info",
            platforms: vec!["windows"],
            description: "Retrieve basic information about the physical disks of a system.",
        },
    );
    tables.insert(
        "dns_cache",
        OsqueryTable {
            name: "dns_cache",
            platforms: vec!["windows"],
            description: "Enumerate the DNS cache using the undocumented DnsGetCacheDataTable function in dnsapi.dll.",
        },
    );
    tables.insert(
        "dns_lookup_events",
        OsqueryTable {
            name: "dns_lookup_events",
            platforms: vec!["windows"],
            description: "DNS lookups performed through the Windows DNS stack.",
        },
    );
    tables.insert(
        "dns_resolvers",
        OsqueryTable {
            name: "dns_resolvers",
            platforms: vec!["darwin", "linux"],
            description: "Resolvers used by this host. Note: On Windows this data is available in the interface_details table.",
        },
    );
    tables.insert(
        "docker_container_envs",
        OsqueryTable {
            name: "docker_container_envs",
            platforms: vec!["darwin", "linux"],
            description: "Docker container environment variables.",
        },
    );
    tables.insert(
        "docker_container_fs_changes",
        OsqueryTable {
            name: "docker_container_fs_changes",
            platforms: vec!["darwin", "linux"],
            description: "Changes to files or directories on container's filesystem.",
        },
    );
    tables.insert(
        "docker_container_labels",
        OsqueryTable {
            name: "docker_container_labels",
            platforms: vec!["darwin", "linux"],
            description: "Docker container labels.",
        },
    );
    tables.insert(
        "docker_container_mounts",
        OsqueryTable {
            name: "docker_container_mounts",
            platforms: vec!["darwin", "linux"],
            description: "Docker container mounts.",
        },
    );
    tables.insert(
        "docker_container_networks",
        OsqueryTable {
            name: "docker_container_networks",
            platforms: vec!["darwin", "linux"],
            description: "Docker container networks.",
        },
    );
    tables.insert(
        "docker_container_ports",
        OsqueryTable {
            name: "docker_container_ports",
            platforms: vec!["darwin", "linux"],
            description: "Docker container ports.",
        },
    );
    tables.insert(
        "docker_container_processes",
        OsqueryTable {
            name: "docker_container_processes",
            platforms: vec!["darwin", "linux"],
            description: "Docker container processes.",
        },
    );
    tables.insert(
        "docker_container_stats",
        OsqueryTable {
            name: "docker_container_stats",
            platforms: vec!["darwin", "linux"],
            description: "Docker container statistics. Queries on this table take at least one second.",
        },
    );
    tables.insert(
        "docker_containers",
        OsqueryTable {
            name: "docker_containers",
            platforms: vec!["darwin", "linux"],
            description: "Docker containers information.",
        },
    );
    tables.insert(
        "docker_image_history",
        OsqueryTable {
            name: "docker_image_history",
            platforms: vec!["darwin", "linux"],
            description: "Docker image history information.",
        },
    );
    tables.insert(
        "docker_image_labels",
        OsqueryTable {
            name: "docker_image_labels",
            platforms: vec!["darwin", "linux"],
            description: "Docker image labels.",
        },
    );
    tables.insert(
        "docker_image_layers",
        OsqueryTable {
            name: "docker_image_layers",
            platforms: vec!["darwin", "linux"],
            description: "Docker image layers information.",
        },
    );
    tables.insert(
        "docker_images",
        OsqueryTable {
            name: "docker_images",
            platforms: vec!["darwin", "linux"],
            description: "Docker images information.",
        },
    );
    tables.insert(
        "docker_info",
        OsqueryTable {
            name: "docker_info",
            platforms: vec!["darwin", "linux"],
            description: "Docker system information.",
        },
    );
    tables.insert(
        "docker_network_labels",
        OsqueryTable {
            name: "docker_network_labels",
            platforms: vec!["darwin", "linux"],
            description: "Docker network labels.",
        },
    );
    tables.insert(
        "docker_networks",
        OsqueryTable {
            name: "docker_networks",
            platforms: vec!["darwin", "linux"],
            description: "Docker networks information.",
        },
    );
    tables.insert(
        "docker_version",
        OsqueryTable {
            name: "docker_version",
            platforms: vec!["darwin", "linux"],
            description: "Docker version information.",
        },
    );
    tables.insert(
        "docker_volume_labels",
        OsqueryTable {
            name: "docker_volume_labels",
            platforms: vec!["darwin", "linux"],
            description: "Docker volume labels.",
        },
    );
    tables.insert(
        "docker_volumes",
        OsqueryTable {
            name: "docker_volumes",
            platforms: vec!["darwin", "linux"],
            description: "Docker volumes information.",
        },
    );
    tables.insert(
        "drivers",
        OsqueryTable {
            name: "drivers",
            platforms: vec!["windows"],
            description: "Details for in-use Windows device drivers. This does not display installed but unused drivers.",
        },
    );
    tables.insert(
        "ec2_instance_metadata",
        OsqueryTable {
            name: "ec2_instance_metadata",
            platforms: vec!["darwin", "linux", "windows"],
            description: "EC2 instance metadata.",
        },
    );
    tables.insert(
        "ec2_instance_tags",
        OsqueryTable {
            name: "ec2_instance_tags",
            platforms: vec!["darwin", "linux", "windows"],
            description: "EC2 instance tag key value pairs.",
        },
    );
    tables.insert(
        "es_process_events",
        OsqueryTable {
            name: "es_process_events",
            platforms: vec!["darwin"],
            description: "Process execution events from EndpointSecurity.",
        },
    );
    tables.insert(
        "es_process_file_events",
        OsqueryTable {
            name: "es_process_file_events",
            platforms: vec!["darwin"],
            description: "File integrity monitoring events from EndpointSecurity including process context.",
        },
    );
    tables.insert(
        "etc_hosts",
        OsqueryTable {
            name: "etc_hosts",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Line-parsed /etc/hosts.",
        },
    );
    tables.insert(
        "etc_protocols",
        OsqueryTable {
            name: "etc_protocols",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Line-parsed /etc/protocols.",
        },
    );
    tables.insert(
        "etc_services",
        OsqueryTable {
            name: "etc_services",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Line-parsed /etc/services.",
        },
    );
    tables.insert(
        "event_taps",
        OsqueryTable {
            name: "event_taps",
            platforms: vec!["darwin"],
            description: "Returns information about installed event taps.",
        },
    );
    tables.insert(
        "extended_attributes",
        OsqueryTable {
            name: "extended_attributes",
            platforms: vec!["darwin", "linux"],
            description: "Returns the extended attributes for files (similar to Windows ADS).",
        },
    );
    tables.insert(
        "fan_speed_sensors",
        OsqueryTable {
            name: "fan_speed_sensors",
            platforms: vec!["darwin"],
            description: "Fan speeds.",
        },
    );
    tables.insert(
        "file",
        OsqueryTable {
            name: "file",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Interactive filesystem attributes and metadata.",
        },
    );
    tables.insert(
        "file_events",
        OsqueryTable {
            name: "file_events",
            platforms: vec!["darwin", "linux"],
            description: "Track time/action changes to files specified in configuration data.",
        },
    );
    tables.insert(
        "filevault_status",
        OsqueryTable {
            name: "filevault_status",
            platforms: vec!["darwin"],
            description: "FileVault disk encryption status (Fleet helper for macOS).",
        },
    );
    tables.insert(
        "firefox_addons",
        OsqueryTable {
            name: "firefox_addons",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Firefox browser extensions, webapps, and addons.",
        },
    );
    tables.insert(
        "gatekeeper",
        OsqueryTable {
            name: "gatekeeper",
            platforms: vec!["darwin"],
            description: "macOS Gatekeeper Details.",
        },
    );
    tables.insert(
        "gatekeeper_approved_apps",
        OsqueryTable {
            name: "gatekeeper_approved_apps",
            platforms: vec!["darwin"],
            description: "Gatekeeper apps a user has allowed to run.",
        },
    );
    tables.insert(
        "groups",
        OsqueryTable {
            name: "groups",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Local system groups.",
        },
    );
    tables.insert(
        "hardware_events",
        OsqueryTable {
            name: "hardware_events",
            platforms: vec!["darwin", "linux"],
            description: "Hardware (PCI/USB/HID) events from UDEV or IOKit.",
        },
    );
    tables.insert(
        "hash",
        OsqueryTable {
            name: "hash",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Filesystem hash data.",
        },
    );
    tables.insert(
        "homebrew_packages",
        OsqueryTable {
            name: "homebrew_packages",
            platforms: vec!["darwin"],
            description: "The installed homebrew package database.",
        },
    );
    tables.insert(
        "ibridge_info",
        OsqueryTable {
            name: "ibridge_info",
            platforms: vec!["darwin"],
            description: "Information about the Apple iBridge hardware controller.",
        },
    );
    tables.insert(
        "ie_extensions",
        OsqueryTable {
            name: "ie_extensions",
            platforms: vec!["windows"],
            description: "Internet Explorer browser extensions.",
        },
    );
    tables.insert(
        "installed_applications",
        OsqueryTable {
            name: "installed_applications",
            platforms: vec!["darwin"],
            description: "Installed macOS applications (Fleet alias for the `apps` table).",
        },
    );
    tables.insert(
        "intel_me_info",
        OsqueryTable {
            name: "intel_me_info",
            platforms: vec!["linux", "windows"],
            description: "Intel ME/CSE Info.",
        },
    );
    tables.insert(
        "interface_addresses",
        OsqueryTable {
            name: "interface_addresses",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Network interfaces and relevant metadata.",
        },
    );
    tables.insert(
        "interface_details",
        OsqueryTable {
            name: "interface_details",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Detailed information and stats of network interfaces.",
        },
    );
    tables.insert(
        "interface_ipv6",
        OsqueryTable {
            name: "interface_ipv6",
            platforms: vec!["darwin", "linux"],
            description: "IPv6 configuration and stats of network interfaces.",
        },
    );
    tables.insert(
        "iokit_devicetree",
        OsqueryTable {
            name: "iokit_devicetree",
            platforms: vec!["darwin"],
            description: "The IOKit registry matching the DeviceTree plane.",
        },
    );
    tables.insert(
        "iokit_registry",
        OsqueryTable {
            name: "iokit_registry",
            platforms: vec!["darwin"],
            description: "The full IOKit registry without selecting a plane.",
        },
    );
    tables.insert(
        "iptables",
        OsqueryTable {
            name: "iptables",
            platforms: vec!["linux"],
            description: "Linux IP packet filtering and NAT tool.",
        },
    );
    tables.insert(
        "jetbrains_plugins",
        OsqueryTable {
            name: "jetbrains_plugins",
            platforms: vec!["darwin", "linux", "windows"],
            description: "JetBrains IDEs plugins.",
        },
    );
    tables.insert(
        "kernel_extensions",
        OsqueryTable {
            name: "kernel_extensions",
            platforms: vec!["darwin"],
            description: "macOS's kernel extensions, both loaded and within the load search path.",
        },
    );
    tables.insert(
        "kernel_info",
        OsqueryTable {
            name: "kernel_info",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Basic active kernel information.",
        },
    );
    tables.insert(
        "kernel_keys",
        OsqueryTable {
            name: "kernel_keys",
            platforms: vec!["linux"],
            description: "List of security data, authentication keys and encryption keys.",
        },
    );
    tables.insert(
        "kernel_modules",
        OsqueryTable {
            name: "kernel_modules",
            platforms: vec!["linux"],
            description: "Linux kernel modules both loaded and within the load search path.",
        },
    );
    tables.insert(
        "kernel_panics",
        OsqueryTable {
            name: "kernel_panics",
            platforms: vec!["darwin"],
            description: "System kernel panic logs.",
        },
    );
    tables.insert(
        "keychain_acls",
        OsqueryTable {
            name: "keychain_acls",
            platforms: vec!["darwin"],
            description: "Applications that have ACL entries in the keychain. NOTE: osquery limits frequent access to keychain files. This limit is controlled by keychain_access_interval flag.",
        },
    );
    tables.insert(
        "keychain_items",
        OsqueryTable {
            name: "keychain_items",
            platforms: vec!["darwin"],
            description: "Generic details about keychain items. NOTE: osquery limits frequent access to keychain files. This limit is controlled by keychain_access_interval flag.",
        },
    );
    tables.insert(
        "known_hosts",
        OsqueryTable {
            name: "known_hosts",
            platforms: vec!["darwin", "linux"],
            description: "A line-delimited known_hosts table.",
        },
    );
    tables.insert(
        "kva_speculative_info",
        OsqueryTable {
            name: "kva_speculative_info",
            platforms: vec!["windows"],
            description: "Display kernel virtual address and speculative execution information for the system.",
        },
    );
    tables.insert(
        "last",
        OsqueryTable {
            name: "last",
            platforms: vec!["darwin", "linux"],
            description: "System logins and logouts.",
        },
    );
    tables.insert(
        "launchd",
        OsqueryTable {
            name: "launchd",
            platforms: vec!["darwin"],
            description: "LaunchAgents and LaunchDaemons from default search paths.",
        },
    );
    tables.insert(
        "launchd_overrides",
        OsqueryTable {
            name: "launchd_overrides",
            platforms: vec!["darwin"],
            description: "Override keys, per user, for LaunchDaemons and Agents.",
        },
    );
    tables.insert(
        "listening_ports",
        OsqueryTable {
            name: "listening_ports",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Processes with listening (bound) network sockets/ports.",
        },
    );
    tables.insert(
        "load_average",
        OsqueryTable {
            name: "load_average",
            platforms: vec!["darwin", "linux"],
            description: "Displays information about the system wide load averages.",
        },
    );
    tables.insert(
        "location_services",
        OsqueryTable {
            name: "location_services",
            platforms: vec!["darwin"],
            description: "Reports the status of the Location Services feature of the OS.",
        },
    );
    tables.insert(
        "logged_in_users",
        OsqueryTable {
            name: "logged_in_users",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Users with an active shell on the system.",
        },
    );
    tables.insert(
        "logical_drives",
        OsqueryTable {
            name: "logical_drives",
            platforms: vec!["windows"],
            description: "Details for logical drives on the system. A logical drive generally represents a single partition.",
        },
    );
    tables.insert(
        "logon_sessions",
        OsqueryTable {
            name: "logon_sessions",
            platforms: vec!["windows"],
            description: "Windows Logon Session.",
        },
    );
    tables.insert(
        "lxd_certificates",
        OsqueryTable {
            name: "lxd_certificates",
            platforms: vec!["linux"],
            description: "LXD certificates information.",
        },
    );
    tables.insert(
        "lxd_cluster",
        OsqueryTable {
            name: "lxd_cluster",
            platforms: vec!["linux"],
            description: "LXD cluster information.",
        },
    );
    tables.insert(
        "lxd_cluster_members",
        OsqueryTable {
            name: "lxd_cluster_members",
            platforms: vec!["linux"],
            description: "LXD cluster members information.",
        },
    );
    tables.insert(
        "lxd_images",
        OsqueryTable {
            name: "lxd_images",
            platforms: vec!["linux"],
            description: "LXD images information.",
        },
    );
    tables.insert(
        "lxd_instance_config",
        OsqueryTable {
            name: "lxd_instance_config",
            platforms: vec!["linux"],
            description: "LXD instance configuration information.",
        },
    );
    tables.insert(
        "lxd_instance_devices",
        OsqueryTable {
            name: "lxd_instance_devices",
            platforms: vec!["linux"],
            description: "LXD instance devices information.",
        },
    );
    tables.insert(
        "lxd_instances",
        OsqueryTable {
            name: "lxd_instances",
            platforms: vec!["linux"],
            description: "LXD instances information.",
        },
    );
    tables.insert(
        "lxd_networks",
        OsqueryTable {
            name: "lxd_networks",
            platforms: vec!["linux"],
            description: "LXD network information.",
        },
    );
    tables.insert(
        "lxd_storage_pools",
        OsqueryTable {
            name: "lxd_storage_pools",
            platforms: vec!["linux"],
            description: "LXD storage pool information.",
        },
    );
    tables.insert(
        "magic",
        OsqueryTable {
            name: "magic",
            platforms: vec!["darwin", "linux"],
            description: "Magic number recognition library table.",
        },
    );
    tables.insert(
        "managed_policies",
        OsqueryTable {
            name: "managed_policies",
            platforms: vec!["darwin"],
            description: "The managed configuration policies from AD, MDM, MCX, etc.",
        },
    );
    tables.insert(
        "md_devices",
        OsqueryTable {
            name: "md_devices",
            platforms: vec!["linux"],
            description: "Software RAID array settings.",
        },
    );
    tables.insert(
        "md_drives",
        OsqueryTable {
            name: "md_drives",
            platforms: vec!["linux"],
            description: "Drive devices used for Software RAID.",
        },
    );
    tables.insert(
        "md_personalities",
        OsqueryTable {
            name: "md_personalities",
            platforms: vec!["linux"],
            description: "Software RAID setting supported by the kernel.",
        },
    );
    tables.insert(
        "mdfind",
        OsqueryTable {
            name: "mdfind",
            platforms: vec!["darwin"],
            description: "Run searches against the spotlight database.",
        },
    );
    tables.insert(
        "mdls",
        OsqueryTable {
            name: "mdls",
            platforms: vec!["darwin"],
            description: "Query file metadata in the Spotlight database.",
        },
    );
    tables.insert(
        "mdm",
        OsqueryTable {
            name: "mdm",
            platforms: vec!["darwin"],
            description: "macOS MDM enrollment status (Fleet extension table).",
        },
    );
    tables.insert(
        "memory_array_mapped_addresses",
        OsqueryTable {
            name: "memory_array_mapped_addresses",
            platforms: vec!["darwin", "linux"],
            description: "Data associated for address mapping of physical memory arrays.",
        },
    );
    tables.insert(
        "memory_arrays",
        OsqueryTable {
            name: "memory_arrays",
            platforms: vec!["darwin", "linux"],
            description: "Data associated with collection of memory devices that operate to form a memory address.",
        },
    );
    tables.insert(
        "memory_device_mapped_addresses",
        OsqueryTable {
            name: "memory_device_mapped_addresses",
            platforms: vec!["darwin", "linux"],
            description: "Data associated for address mapping of physical memory devices.",
        },
    );
    tables.insert(
        "memory_devices",
        OsqueryTable {
            name: "memory_devices",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Physical memory device (type 17) information retrieved from SMBIOS.",
        },
    );
    tables.insert(
        "memory_error_info",
        OsqueryTable {
            name: "memory_error_info",
            platforms: vec!["darwin", "linux"],
            description: "Data associated with errors of a physical memory array.",
        },
    );
    tables.insert(
        "memory_info",
        OsqueryTable {
            name: "memory_info",
            platforms: vec!["linux"],
            description: "Main memory information in bytes.",
        },
    );
    tables.insert(
        "memory_map",
        OsqueryTable {
            name: "memory_map",
            platforms: vec!["linux"],
            description: "OS memory region map.",
        },
    );
    tables.insert(
        "mounts",
        OsqueryTable {
            name: "mounts",
            platforms: vec!["darwin", "linux"],
            description: "System mounted devices and filesystems (not process specific).",
        },
    );
    tables.insert(
        "msr",
        OsqueryTable {
            name: "msr",
            platforms: vec!["linux"],
            description: "Various pieces of data stored in the model specific register per processor. NOTE: the msr kernel module must be enabled, and osquery must be run as root.",
        },
    );
    tables.insert(
        "nfs_shares",
        OsqueryTable {
            name: "nfs_shares",
            platforms: vec!["darwin"],
            description: "NFS shares exported by the host.",
        },
    );
    tables.insert(
        "npm_packages",
        OsqueryTable {
            name: "npm_packages",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Node packages installed in a system.",
        },
    );
    tables.insert(
        "ntdomains",
        OsqueryTable {
            name: "ntdomains",
            platforms: vec!["windows"],
            description: "Display basic NT domain information of a Windows machine.",
        },
    );
    tables.insert(
        "ntfs_acl_permissions",
        OsqueryTable {
            name: "ntfs_acl_permissions",
            platforms: vec!["windows"],
            description: "Retrieve NTFS ACL permission information for files and directories.",
        },
    );
    tables.insert(
        "ntfs_journal_events",
        OsqueryTable {
            name: "ntfs_journal_events",
            platforms: vec!["windows"],
            description: "Track time/action changes to files specified in configuration data.",
        },
    );
    tables.insert(
        "nvram",
        OsqueryTable {
            name: "nvram",
            platforms: vec!["darwin"],
            description: "Apple NVRAM variable listing.",
        },
    );
    tables.insert(
        "oem_strings",
        OsqueryTable {
            name: "oem_strings",
            platforms: vec!["darwin", "linux"],
            description: "OEM defined strings retrieved from SMBIOS.",
        },
    );
    tables.insert(
        "office_mru",
        OsqueryTable {
            name: "office_mru",
            platforms: vec!["windows"],
            description: "View recently opened Office documents.",
        },
    );
    tables.insert(
        "os_version",
        OsqueryTable {
            name: "os_version",
            platforms: vec!["darwin", "linux", "windows"],
            description: "A single row containing the operating system name and version.",
        },
    );
    tables.insert(
        "osquery_events",
        OsqueryTable {
            name: "osquery_events",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Information about the event publishers and subscribers.",
        },
    );
    tables.insert(
        "osquery_extensions",
        OsqueryTable {
            name: "osquery_extensions",
            platforms: vec!["darwin", "linux", "windows"],
            description: "List of active osquery extensions.",
        },
    );
    tables.insert(
        "osquery_flags",
        OsqueryTable {
            name: "osquery_flags",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Configurable flags that modify osquery's behavior.",
        },
    );
    tables.insert(
        "osquery_info",
        OsqueryTable {
            name: "osquery_info",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Top level information about the running version of osquery.",
        },
    );
    tables.insert(
        "osquery_packs",
        OsqueryTable {
            name: "osquery_packs",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Information about the current query packs that are loaded in osquery.",
        },
    );
    tables.insert(
        "osquery_registry",
        OsqueryTable {
            name: "osquery_registry",
            platforms: vec!["darwin", "linux", "windows"],
            description: "List the osquery registry plugins.",
        },
    );
    tables.insert(
        "osquery_schedule",
        OsqueryTable {
            name: "osquery_schedule",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Information about the current queries that are scheduled in osquery.",
        },
    );
    tables.insert(
        "package_bom",
        OsqueryTable {
            name: "package_bom",
            platforms: vec!["darwin"],
            description: "macOS package bill of materials (BOM) file list.",
        },
    );
    tables.insert(
        "package_install_history",
        OsqueryTable {
            name: "package_install_history",
            platforms: vec!["darwin"],
            description: "macOS package install history.",
        },
    );
    tables.insert(
        "package_receipts",
        OsqueryTable {
            name: "package_receipts",
            platforms: vec!["darwin"],
            description: "macOS package receipt details.",
        },
    );
    tables.insert(
        "password_policy",
        OsqueryTable {
            name: "password_policy",
            platforms: vec!["darwin"],
            description: "OpenDirectory account policies for macOS including password content, authentication, and password change policies.",
        },
    );
    tables.insert(
        "patches",
        OsqueryTable {
            name: "patches",
            platforms: vec!["windows"],
            description: "Lists all the patches applied. Note: This does not include patches applied via MSI or downloaded from Windows Update (e.g. Service Packs).",
        },
    );
    tables.insert(
        "pci_devices",
        OsqueryTable {
            name: "pci_devices",
            platforms: vec!["darwin", "linux"],
            description: "PCI devices active on the host system.",
        },
    );
    tables.insert(
        "physical_disk_performance",
        OsqueryTable {
            name: "physical_disk_performance",
            platforms: vec!["windows"],
            description: "Provides provides raw data from performance counters that monitor hard or fixed disk drives on the system.",
        },
    );
    tables.insert(
        "pipes",
        OsqueryTable {
            name: "pipes",
            platforms: vec!["windows"],
            description: "Named and Anonymous pipes.",
        },
    );
    tables.insert(
        "platform_info",
        OsqueryTable {
            name: "platform_info",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Information about EFI/UEFI/ROM and platform/boot.",
        },
    );
    tables.insert(
        "plist",
        OsqueryTable {
            name: "plist",
            platforms: vec!["darwin"],
            description: "Read and parse a plist file.",
        },
    );
    tables.insert(
        "portage_keywords",
        OsqueryTable {
            name: "portage_keywords",
            platforms: vec!["linux"],
            description: "A summary about portage configurations like keywords, mask and unmask.",
        },
    );
    tables.insert(
        "portage_packages",
        OsqueryTable {
            name: "portage_packages",
            platforms: vec!["linux"],
            description: "List of currently installed packages.",
        },
    );
    tables.insert(
        "portage_use",
        OsqueryTable {
            name: "portage_use",
            platforms: vec!["linux"],
            description: "List of enabled portage USE values for specific package.",
        },
    );
    tables.insert(
        "power_sensors",
        OsqueryTable {
            name: "power_sensors",
            platforms: vec!["darwin"],
            description: "Machine power (currents, voltages, wattages, etc) sensors.",
        },
    );
    tables.insert(
        "powershell_events",
        OsqueryTable {
            name: "powershell_events",
            platforms: vec!["windows"],
            description: "Powershell script blocks reconstructed to their full script content, this table requires script block logging to be enabled.",
        },
    );
    tables.insert(
        "preferences",
        OsqueryTable {
            name: "preferences",
            platforms: vec!["darwin"],
            description: "macOS defaults and managed preferences.",
        },
    );
    tables.insert(
        "prefetch",
        OsqueryTable {
            name: "prefetch",
            platforms: vec!["windows"],
            description: "Prefetch files show metadata related to file execution.",
        },
    );
    tables.insert(
        "process_envs",
        OsqueryTable {
            name: "process_envs",
            platforms: vec!["darwin", "linux"],
            description: "A key/value table of environment variables for each process.",
        },
    );
    tables.insert(
        "process_etw_events",
        OsqueryTable {
            name: "process_etw_events",
            platforms: vec!["windows"],
            description: "Windows process execution events.",
        },
    );
    tables.insert(
        "process_events",
        OsqueryTable {
            name: "process_events",
            platforms: vec!["darwin", "linux"],
            description: "Track time/action process executions.",
        },
    );
    tables.insert(
        "process_file_events",
        OsqueryTable {
            name: "process_file_events",
            platforms: vec!["linux"],
            description: "A File Integrity Monitor implementation using the audit service.",
        },
    );
    tables.insert(
        "process_memory_map",
        OsqueryTable {
            name: "process_memory_map",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Process memory mapped files and pseudo device/regions.",
        },
    );
    tables.insert(
        "process_namespaces",
        OsqueryTable {
            name: "process_namespaces",
            platforms: vec!["linux"],
            description: "Linux namespaces for processes running on the host system.",
        },
    );
    tables.insert(
        "process_open_files",
        OsqueryTable {
            name: "process_open_files",
            platforms: vec!["darwin", "linux"],
            description: "File descriptors for each process.",
        },
    );
    tables.insert(
        "process_open_handles",
        OsqueryTable {
            name: "process_open_handles",
            platforms: vec!["windows"],
            description: "Enumerate open handles for a specified process.",
        },
    );
    tables.insert(
        "process_open_pipes",
        OsqueryTable {
            name: "process_open_pipes",
            platforms: vec!["linux"],
            description: "Pipes and partner processes for each process.",
        },
    );
    tables.insert(
        "process_open_sockets",
        OsqueryTable {
            name: "process_open_sockets",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Processes which have open network sockets on the system.",
        },
    );
    tables.insert(
        "processes",
        OsqueryTable {
            name: "processes",
            platforms: vec!["darwin", "linux", "windows"],
            description: "All running processes on the host system.",
        },
    );
    tables.insert(
        "programs",
        OsqueryTable {
            name: "programs",
            platforms: vec!["windows"],
            description: "Represents products as they are installed by Windows Installer. A product generally correlates to one installation package on Windows. Some fields may be blank as Windows installation details are left to the discretion of the product author.",
        },
    );
    tables.insert(
        "prometheus_metrics",
        OsqueryTable {
            name: "prometheus_metrics",
            platforms: vec!["darwin", "linux"],
            description: "Retrieve metrics from a Prometheus server.",
        },
    );
    tables.insert(
        "python_packages",
        OsqueryTable {
            name: "python_packages",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Python packages installed in a system. NOTE: when querying on windows, even without a users cross join, all user installed python packages will be returned. This special behavior is to not break original functionality.",
        },
    );
    tables.insert(
        "quicklook_cache",
        OsqueryTable {
            name: "quicklook_cache",
            platforms: vec!["darwin"],
            description: "Files and thumbnails within macOS's Quicklook Cache.",
        },
    );
    tables.insert(
        "recent_files",
        OsqueryTable {
            name: "recent_files",
            platforms: vec!["windows"],
            description: "Recently files (as displayed in Start Menu or File Explorer).",
        },
    );
    tables.insert(
        "registry",
        OsqueryTable {
            name: "registry",
            platforms: vec!["windows"],
            description: "All of the Windows registry hives.",
        },
    );
    tables.insert(
        "routes",
        OsqueryTable {
            name: "routes",
            platforms: vec!["darwin", "linux", "windows"],
            description: "The active route table for the host system.",
        },
    );
    tables.insert(
        "rpm_package_files",
        OsqueryTable {
            name: "rpm_package_files",
            platforms: vec!["linux"],
            description: "Installed files from RPM packages that are currently installed on the system.",
        },
    );
    tables.insert(
        "rpm_packages",
        OsqueryTable {
            name: "rpm_packages",
            platforms: vec!["linux"],
            description: "RPM packages that are currently installed on the host system.",
        },
    );
    tables.insert(
        "running_apps",
        OsqueryTable {
            name: "running_apps",
            platforms: vec!["darwin"],
            description: "macOS applications currently running on the host system.",
        },
    );
    tables.insert(
        "safari_extensions",
        OsqueryTable {
            name: "safari_extensions",
            platforms: vec!["darwin"],
            description: "Safari browser extension details for all users. This table requires Full Disk Access (FDA) permission.",
        },
    );
    tables.insert(
        "sandboxes",
        OsqueryTable {
            name: "sandboxes",
            platforms: vec!["darwin"],
            description: "macOS application sandboxes container details.",
        },
    );
    tables.insert(
        "scheduled_tasks",
        OsqueryTable {
            name: "scheduled_tasks",
            platforms: vec!["windows"],
            description: "Lists all of the tasks in the Windows task scheduler.",
        },
    );
    tables.insert(
        "screenlock",
        OsqueryTable {
            name: "screenlock",
            platforms: vec!["darwin"],
            description: "macOS screenlock status. Note: only fetches results for osquery's current logged-in user context. The user must also have recently logged in.",
        },
    );
    tables.insert(
        "seccomp_events",
        OsqueryTable {
            name: "seccomp_events",
            platforms: vec!["linux"],
            description: "A virtual table that tracks seccomp events.",
        },
    );
    tables.insert(
        "secureboot",
        OsqueryTable {
            name: "secureboot",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Secure Boot UEFI Settings.",
        },
    );
    tables.insert(
        "secureboot_certificates",
        OsqueryTable {
            name: "secureboot_certificates",
            platforms: vec!["linux"],
            description: "X.509 certificates from UEFI Secure Boot signature databases (db and dbx EFI variables). Useful for monitoring CA expiry and adoption of updated certificates (e.g. Microsoft UEFI CA 2023).",
        },
    );
    tables.insert(
        "security_profile_info",
        OsqueryTable {
            name: "security_profile_info",
            platforms: vec!["windows"],
            description: "Information on the security profile of a given system by listing the system Account and Audit Policies. This table mimics the exported securitypolicy output from the secedit tool.",
        },
    );
    tables.insert(
        "selinux_events",
        OsqueryTable {
            name: "selinux_events",
            platforms: vec!["linux"],
            description: "Track SELinux events.",
        },
    );
    tables.insert(
        "selinux_settings",
        OsqueryTable {
            name: "selinux_settings",
            platforms: vec!["linux"],
            description: "Track active SELinux settings.",
        },
    );
    tables.insert(
        "services",
        OsqueryTable {
            name: "services",
            platforms: vec!["windows"],
            description: "Lists all installed Windows services and their relevant data.",
        },
    );
    tables.insert(
        "shadow",
        OsqueryTable {
            name: "shadow",
            platforms: vec!["linux"],
            description: "Local system users encrypted passwords and related information. Please note, that you usually need superuser rights to access `/etc/shadow`.",
        },
    );
    tables.insert(
        "shared_folders",
        OsqueryTable {
            name: "shared_folders",
            platforms: vec!["darwin"],
            description: "Folders available to others via SMB or AFP.",
        },
    );
    tables.insert(
        "shared_memory",
        OsqueryTable {
            name: "shared_memory",
            platforms: vec!["linux"],
            description: "OS shared memory regions.",
        },
    );
    tables.insert(
        "shared_resources",
        OsqueryTable {
            name: "shared_resources",
            platforms: vec!["windows"],
            description: "Displays shared resources on a computer system running Windows. This may be a disk drive, printer, interprocess communication, or other sharable device.",
        },
    );
    tables.insert(
        "sharing_preferences",
        OsqueryTable {
            name: "sharing_preferences",
            platforms: vec!["darwin"],
            description: "macOS Sharing preferences.",
        },
    );
    tables.insert(
        "shell_history",
        OsqueryTable {
            name: "shell_history",
            platforms: vec!["darwin", "linux"],
            description: "A line-delimited (command) table of per-user .*_history data.",
        },
    );
    tables.insert(
        "shellbags",
        OsqueryTable {
            name: "shellbags",
            platforms: vec!["windows"],
            description: "Shows directories accessed via Windows Explorer.",
        },
    );
    tables.insert(
        "shimcache",
        OsqueryTable {
            name: "shimcache",
            platforms: vec!["windows"],
            description: "Application Compatibility Cache, contains artifacts of execution.",
        },
    );
    tables.insert(
        "signature",
        OsqueryTable {
            name: "signature",
            platforms: vec!["darwin"],
            description: "File (executable, bundle, installer, disk) code signing status.",
        },
    );
    tables.insert(
        "sip_config",
        OsqueryTable {
            name: "sip_config",
            platforms: vec!["darwin"],
            description: "Apple's System Integrity Protection (rootless) status.",
        },
    );
    tables.insert(
        "smbios_tables",
        OsqueryTable {
            name: "smbios_tables",
            platforms: vec!["darwin", "linux"],
            description: "BIOS (DMI) structure common details and content.",
        },
    );
    tables.insert(
        "smc_keys",
        OsqueryTable {
            name: "smc_keys",
            platforms: vec!["darwin"],
            description: "Apple's system management controller keys.",
        },
    );
    tables.insert(
        "socket_events",
        OsqueryTable {
            name: "socket_events",
            platforms: vec!["darwin", "linux"],
            description: "Track network socket bind, connect, and accepts.",
        },
    );
    tables.insert(
        "ssh_configs",
        OsqueryTable {
            name: "ssh_configs",
            platforms: vec!["darwin", "linux", "windows"],
            description: "A table of parsed ssh_configs.",
        },
    );
    tables.insert(
        "startup_items",
        OsqueryTable {
            name: "startup_items",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Applications and binaries set as startup items.",
        },
    );
    tables.insert(
        "sudoers",
        OsqueryTable {
            name: "sudoers",
            platforms: vec!["darwin", "linux"],
            description: "Rules for running commands as other users via sudo.",
        },
    );
    tables.insert(
        "suid_bin",
        OsqueryTable {
            name: "suid_bin",
            platforms: vec!["darwin", "linux"],
            description: "suid binaries in common locations.",
        },
    );
    tables.insert(
        "syslog_events",
        OsqueryTable {
            name: "syslog_events",
            platforms: vec!["linux"],
            description: "",
        },
    );
    tables.insert(
        "system_controls",
        OsqueryTable {
            name: "system_controls",
            platforms: vec!["darwin", "linux"],
            description: "sysctl names, values, and settings information.",
        },
    );
    tables.insert(
        "system_extensions",
        OsqueryTable {
            name: "system_extensions",
            platforms: vec!["darwin"],
            description: "macOS (>= 10.15) system extension table.",
        },
    );
    tables.insert(
        "system_info",
        OsqueryTable {
            name: "system_info",
            platforms: vec!["darwin", "linux", "windows"],
            description: "System information for identification.",
        },
    );
    tables.insert(
        "system_profiler",
        OsqueryTable {
            name: "system_profiler",
            platforms: vec!["darwin"],
            description: "Query system_profiler data types and return the full result as JSON. Returns only the data types specified in the constraints. See available data types with `system_profiler -listDataTypes`.",
        },
    );
    tables.insert(
        "systemd_units",
        OsqueryTable {
            name: "systemd_units",
            platforms: vec!["linux"],
            description: "Track systemd units.",
        },
    );
    tables.insert(
        "temperature_sensors",
        OsqueryTable {
            name: "temperature_sensors",
            platforms: vec!["darwin"],
            description: "Machine's temperature sensors.",
        },
    );
    tables.insert(
        "time",
        OsqueryTable {
            name: "time",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Track current date and time in UTC.",
        },
    );
    tables.insert(
        "time_machine_backups",
        OsqueryTable {
            name: "time_machine_backups",
            platforms: vec!["darwin"],
            description: "Backups to drives using TimeMachine. This table requires Full Disk Access (FDA) permission.",
        },
    );
    tables.insert(
        "time_machine_destinations",
        OsqueryTable {
            name: "time_machine_destinations",
            platforms: vec!["darwin"],
            description: "Locations backed up to using Time Machine. This table requires Full Disk Access (FDA) permission.",
        },
    );
    tables.insert(
        "tpm_info",
        OsqueryTable {
            name: "tpm_info",
            platforms: vec!["windows"],
            description: "A table that lists the TPM related information.",
        },
    );
    tables.insert(
        "ulimit_info",
        OsqueryTable {
            name: "ulimit_info",
            platforms: vec!["darwin", "linux"],
            description: "System resource usage limits.",
        },
    );
    tables.insert(
        "unified_log",
        OsqueryTable {
            name: "unified_log",
            platforms: vec!["darwin"],
            description: "Queries the OSLog framework for entries in the system log. The maximum number of rows returned is limited for performance issues. Use timestamp > or >= constraints to optimize query performance. This table introduces a new idiom for extracting sequential data in batches using multiple queries, ordered by timestamp. To trigger it, the user should include the condition \"timestamp > -1\", and the table will handle pagination. Note that the saved pagination counter is incremented globally across all queries and table invocations within a query. To avoid multiple table invocations within a query, use only AND and = constraints in WHERE clause.",
        },
    );
    tables.insert(
        "uptime",
        OsqueryTable {
            name: "uptime",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Track time passed since last boot. Some systems track this as calendar time, some as runtime.",
        },
    );
    tables.insert(
        "usb_devices",
        OsqueryTable {
            name: "usb_devices",
            platforms: vec!["darwin", "linux"],
            description: "USB devices that are actively plugged into the host system.",
        },
    );
    tables.insert(
        "user_events",
        OsqueryTable {
            name: "user_events",
            platforms: vec!["darwin", "linux"],
            description: "Track user events from the audit framework.",
        },
    );
    tables.insert(
        "user_groups",
        OsqueryTable {
            name: "user_groups",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Local system user group relationships.",
        },
    );
    tables.insert(
        "user_interaction_events",
        OsqueryTable {
            name: "user_interaction_events",
            platforms: vec!["darwin"],
            description: "Track user interaction events from macOS' event tapping framework.",
        },
    );
    tables.insert(
        "user_ssh_keys",
        OsqueryTable {
            name: "user_ssh_keys",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Returns the private keys in the users ~/.ssh directory and whether or not they are encrypted.",
        },
    );
    tables.insert(
        "userassist",
        OsqueryTable {
            name: "userassist",
            platforms: vec!["windows"],
            description: "UserAssist Registry Key tracks when a user executes an application from Windows Explorer.",
        },
    );
    tables.insert(
        "users",
        OsqueryTable {
            name: "users",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Local user accounts (including domain accounts that have logged on locally (Windows)).",
        },
    );
    tables.insert(
        "video_info",
        OsqueryTable {
            name: "video_info",
            platforms: vec!["windows"],
            description: "Retrieve video card information of the machine.",
        },
    );
    tables.insert(
        "virtual_memory_info",
        OsqueryTable {
            name: "virtual_memory_info",
            platforms: vec!["darwin"],
            description: "Darwin Virtual Memory statistics.",
        },
    );
    tables.insert(
        "vscode_extensions",
        OsqueryTable {
            name: "vscode_extensions",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Lists all vscode extensions.",
        },
    );
    tables.insert(
        "wifi_networks",
        OsqueryTable {
            name: "wifi_networks",
            platforms: vec!["darwin"],
            description: "macOS known/remembered Wi-Fi networks list.",
        },
    );
    tables.insert(
        "wifi_status",
        OsqueryTable {
            name: "wifi_status",
            platforms: vec!["darwin"],
            description: "macOS current WiFi status. This table requires Full Disk Access (FDA) permission to retrieve network_name.",
        },
    );
    tables.insert(
        "wifi_survey",
        OsqueryTable {
            name: "wifi_survey",
            platforms: vec!["darwin"],
            description: "Scan for nearby WiFi networks.",
        },
    );
    tables.insert(
        "winbaseobj",
        OsqueryTable {
            name: "winbaseobj",
            platforms: vec!["windows"],
            description: "Lists named Windows objects in the default object directories, across all terminal services sessions. Example Windows object types include Mutexes, Events, Jobs and Semaphors.",
        },
    );
    tables.insert(
        "windows_crashes",
        OsqueryTable {
            name: "windows_crashes",
            platforms: vec!["windows"],
            description: "Extracted information from Windows crash logs (Minidumps).",
        },
    );
    tables.insert(
        "windows_eventlog",
        OsqueryTable {
            name: "windows_eventlog",
            platforms: vec!["windows"],
            description: "Table for querying all recorded Windows event logs.",
        },
    );
    tables.insert(
        "windows_events",
        OsqueryTable {
            name: "windows_events",
            platforms: vec!["windows"],
            description: "Windows Event logs.",
        },
    );
    tables.insert(
        "windows_firewall_rules",
        OsqueryTable {
            name: "windows_firewall_rules",
            platforms: vec!["windows"],
            description: "Provides the list of Windows firewall rules.",
        },
    );
    tables.insert(
        "windows_optional_features",
        OsqueryTable {
            name: "windows_optional_features",
            platforms: vec!["windows"],
            description: "Lists names and installation states of windows features. Maps to Win32_OptionalFeature WMI class.",
        },
    );
    tables.insert(
        "windows_search",
        OsqueryTable {
            name: "windows_search",
            platforms: vec!["windows"],
            description: "Run searches against the Windows system index database using Advanced Query Syntax. See https://learn.microsoft.com/en-us/windows/win32/search/-search-3x-advancedquerysyntax for details.",
        },
    );
    tables.insert(
        "windows_security_center",
        OsqueryTable {
            name: "windows_security_center",
            platforms: vec!["windows"],
            description: "The health status of Window Security features. Health values can be \"Good\", \"Poor\". \"Snoozed\", \"Not Monitored\", and \"Error\".",
        },
    );
    tables.insert(
        "windows_security_products",
        OsqueryTable {
            name: "windows_security_products",
            platforms: vec!["windows"],
            description: "Enumeration of registered Windows security products. Note: Not compatible with Windows Server.",
        },
    );
    tables.insert(
        "windows_update_history",
        OsqueryTable {
            name: "windows_update_history",
            platforms: vec!["windows"],
            description: "Provides the history of the windows update events.",
        },
    );
    tables.insert(
        "wmi_bios_info",
        OsqueryTable {
            name: "wmi_bios_info",
            platforms: vec!["windows"],
            description: "Lists important information from the system bios.",
        },
    );
    tables.insert(
        "wmi_cli_event_consumers",
        OsqueryTable {
            name: "wmi_cli_event_consumers",
            platforms: vec!["windows"],
            description: "WMI CommandLineEventConsumer, which can be used for persistence on Windows. See https://www.blackhat.com/docs/us-15/materials/us-15-Graeber-Abusing-Windows-Management-Instrumentation-WMI-To-Build-A-Persistent%20Asynchronous-And-Fileless-Backdoor-wp.pdf for more details.",
        },
    );
    tables.insert(
        "wmi_event_filters",
        OsqueryTable {
            name: "wmi_event_filters",
            platforms: vec!["windows"],
            description: "Lists WMI event filters.",
        },
    );
    tables.insert(
        "wmi_filter_consumer_binding",
        OsqueryTable {
            name: "wmi_filter_consumer_binding",
            platforms: vec!["windows"],
            description: "Lists the relationship between event consumers and filters.",
        },
    );
    tables.insert(
        "wmi_script_event_consumers",
        OsqueryTable {
            name: "wmi_script_event_consumers",
            platforms: vec!["windows"],
            description: "WMI ActiveScriptEventConsumer, which can be used for persistence on Windows. See https://www.blackhat.com/docs/us-15/materials/us-15-Graeber-Abusing-Windows-Management-Instrumentation-WMI-To-Build-A-Persistent%20Asynchronous-And-Fileless-Backdoor-wp.pdf for more details.",
        },
    );
    tables.insert(
        "xprotect_entries",
        OsqueryTable {
            name: "xprotect_entries",
            platforms: vec!["darwin"],
            description: "Database of the machine's XProtect signatures.",
        },
    );
    tables.insert(
        "xprotect_meta",
        OsqueryTable {
            name: "xprotect_meta",
            platforms: vec!["darwin"],
            description: "Database of the machine's XProtect browser-related signatures.",
        },
    );
    tables.insert(
        "xprotect_reports",
        OsqueryTable {
            name: "xprotect_reports",
            platforms: vec!["darwin"],
            description: "Database of XProtect matches (if user generated/sent an XProtect report).",
        },
    );
    tables.insert(
        "yara_events",
        OsqueryTable {
            name: "yara_events",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Track YARA matches for files specified in configuration data.",
        },
    );
    tables.insert(
        "yara_file",
        OsqueryTable {
            name: "yara_file",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Triggers one-off YARA query for files at the specified path. Additionally requires one of `sig_group`, `sigfile`, or `sigrule`.",
        },
    );
    tables.insert(
        "yara_process",
        OsqueryTable {
            name: "yara_process",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Triggers one-off YARA query for process memory of the specified pid. Additionally requires one of `sig_group`, `sigfile`, or `sigrule`.",
        },
    );
    tables.insert(
        "ycloud_instance_metadata",
        OsqueryTable {
            name: "ycloud_instance_metadata",
            platforms: vec!["darwin", "linux", "windows"],
            description: "Yandex.Cloud instance metadata.",
        },
    );
    tables.insert(
        "yum_sources",
        OsqueryTable {
            name: "yum_sources",
            platforms: vec!["linux"],
            description: "Current list of Yum repositories or software channels.",
        },
    );

    tables
});
