//! Fleet's `$FLEET_VAR_*` variables and where Fleet accepts each one.
//!
//! Fleet resolves these on the server, but it validates them on upload: a
//! name it does not know, or one not allowed in that kind of file, fails the
//! apply ("Fleet variable $FLEET_VAR_X is not supported in configuration
//! profiles."). The lists below mirror Fleet's own allow-lists; tests compare
//! them with the ones `schema-extract` reads from the pinned Fleet source
//! (`data/fleet-gitops-keys.json`), so a Fleet change fails the build.

/// One Fleet variable. A name ending in `_` is a prefix that takes a
/// certificate authority name (`$FLEET_VAR_DIGICERT_DATA_MyCA`).
pub struct FleetVar {
    pub name: &'static str,
    pub description: &'static str,
}

/// Every variable Fleet defines (server/fleet: consts typed FleetVarName).
pub const FLEET_VARS: &[FleetVar] = &[
    FleetVar { name: "HOST_HARDWARE_SERIAL", description: "Host hardware serial number" },
    FleetVar { name: "HOST_UUID", description: "Host UUID" },
    FleetVar { name: "HOST_PLATFORM", description: "Host platform (darwin, windows, linux, …)" },
    FleetVar { name: "HOST_END_USER_IDP_USERNAME", description: "End user's IdP username" },
    FleetVar { name: "HOST_END_USER_IDP_USERNAME_LOCAL_PART", description: "Local part of the IdP username (before @)" },
    FleetVar { name: "HOST_END_USER_IDP_FULL_NAME", description: "End user's full name from the IdP" },
    FleetVar { name: "HOST_END_USER_IDP_GROUPS", description: "End user's IdP groups" },
    FleetVar { name: "HOST_END_USER_IDP_DEPARTMENT", description: "End user's IdP department" },
    FleetVar { name: "HOST_END_USER_EMAIL_IDP", description: "End user's email from the IdP (legacy; prefer HOST_END_USER_IDP_USERNAME)" },
    FleetVar { name: "NDES_SCEP_CHALLENGE", description: "NDES SCEP challenge" },
    FleetVar { name: "NDES_SCEP_PROXY_URL", description: "NDES SCEP proxy URL" },
    FleetVar { name: "CERTIFICATE_RENEWAL_ID", description: "Fleet-managed ID for automatic certificate renewal (put it in the certificate's OU)" },
    FleetVar { name: "SCEP_RENEWAL_ID", description: "Legacy name of CERTIFICATE_RENEWAL_ID" },
    FleetVar { name: "SCEP_WINDOWS_CERTIFICATE_ID", description: "Windows SCEP certificate ID" },
    FleetVar { name: "PSSO_DEVICE_REGISTRATION_TOKEN", description: "Per-device Platform SSO registration token (RegistrationToken key only; Fleet Premium)" },
    FleetVar { name: "DIGICERT_DATA_", description: "DigiCert certificate data for the named CA" },
    FleetVar { name: "DIGICERT_PASSWORD_", description: "DigiCert certificate password for the named CA" },
    FleetVar { name: "CUSTOM_SCEP_CHALLENGE_", description: "Custom SCEP challenge for the named CA" },
    FleetVar { name: "CUSTOM_SCEP_PROXY_URL_", description: "Custom SCEP proxy URL for the named CA" },
    FleetVar { name: "SMALLSTEP_SCEP_CHALLENGE_", description: "Smallstep SCEP challenge for the named CA" },
    FleetVar { name: "SMALLSTEP_SCEP_PROXY_URL_", description: "Smallstep SCEP proxy URL for the named CA" },
    FleetVar { name: "HOST_TARGET_OS_VERSION", description: "Fleet-internal (Fleet's own OS-update declaration); not for your files" },
    FleetVar { name: "HOST_TARGET_OS_DEADLINE", description: "Fleet-internal (Fleet's own OS-update declaration); not for your files" },
    FleetVar { name: "PATCH_NOTIFICATION_URL", description: "Fleet-internal (patch notifications); not for your files" },
];

/// Where a variable appears, and therefore which Fleet allow-list applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarContext {
    AppleProfile,
    AppleDeclaration,
    WindowsProfile,
    Script,
}

const IDP_AND_HOST: [&str; 8] = [
    "HOST_END_USER_IDP_DEPARTMENT",
    "HOST_END_USER_IDP_FULL_NAME",
    "HOST_END_USER_IDP_GROUPS",
    "HOST_END_USER_IDP_USERNAME",
    "HOST_END_USER_IDP_USERNAME_LOCAL_PART",
    "HOST_HARDWARE_SERIAL",
    "HOST_PLATFORM",
    "HOST_UUID",
];

/// fleetVarsSupportedInAppleConfigProfiles (server/service/apple_mdm.go).
const APPLE_PROFILE: &[&str] = &[
    "CERTIFICATE_RENEWAL_ID", "HOST_END_USER_EMAIL_IDP", "HOST_END_USER_IDP_DEPARTMENT",
    "HOST_END_USER_IDP_FULL_NAME", "HOST_END_USER_IDP_GROUPS", "HOST_END_USER_IDP_USERNAME",
    "HOST_END_USER_IDP_USERNAME_LOCAL_PART", "HOST_HARDWARE_SERIAL", "HOST_PLATFORM", "HOST_UUID",
    "NDES_SCEP_CHALLENGE", "NDES_SCEP_PROXY_URL", "PSSO_DEVICE_REGISTRATION_TOKEN", "SCEP_RENEWAL_ID",
];
/// fleetVarsSupportedInWindowsProfiles (server/service/windows_mdm_profiles.go).
const WINDOWS_PROFILE: &[&str] = &[
    "CERTIFICATE_RENEWAL_ID", "HOST_END_USER_IDP_DEPARTMENT", "HOST_END_USER_IDP_FULL_NAME",
    "HOST_END_USER_IDP_GROUPS", "HOST_END_USER_IDP_USERNAME", "HOST_END_USER_IDP_USERNAME_LOCAL_PART",
    "HOST_HARDWARE_SERIAL", "HOST_PLATFORM", "HOST_UUID", "NDES_SCEP_CHALLENGE",
    "NDES_SCEP_PROXY_URL", "SCEP_RENEWAL_ID", "SCEP_WINDOWS_CERTIFICATE_ID",
];

// The CA prefixes are accepted by a HasPrefix check in Fleet's code, not by
// the lists (apple_mdm.go validateConfigProfileFleetVariables,
// windows_mdm_profiles.go).
const APPLE_PROFILE_PREFIXES: &[&str] = &[
    "DIGICERT_DATA_", "DIGICERT_PASSWORD_", "CUSTOM_SCEP_CHALLENGE_",
    "CUSTOM_SCEP_PROXY_URL_", "SMALLSTEP_SCEP_CHALLENGE_", "SMALLSTEP_SCEP_PROXY_URL_",
];
const WINDOWS_PROFILE_PREFIXES: &[&str] = &["CUSTOM_SCEP_CHALLENGE_", "CUSTOM_SCEP_PROXY_URL_"];

impl VarContext {
    /// Fleet's allow-list for this context, and the CA prefixes it accepts.
    fn allowed(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            // fleetVarsSupportedInDDMDeclarations, FleetVarsSupportedInScripts
            VarContext::AppleDeclaration | VarContext::Script => (&IDP_AND_HOST, &[]),
            VarContext::AppleProfile => (APPLE_PROFILE, APPLE_PROFILE_PREFIXES),
            VarContext::WindowsProfile => (WINDOWS_PROFILE, WINDOWS_PROFILE_PREFIXES),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VarContext::AppleProfile => "configuration profiles",
            VarContext::AppleDeclaration => "DDM declarations",
            VarContext::WindowsProfile => "Windows profiles",
            VarContext::Script => "scripts",
        }
    }

    /// Whether Fleet accepts `$FLEET_VAR_<name>` here.
    pub fn accepts(self, name: &str) -> bool {
        let (list, prefixes) = self.allowed();
        list.contains(&name) || prefixes.iter().any(|p| name.len() > p.len() && name.starts_with(p))
    }
}

/// Whether Fleet defines `name` at all (a prefix variable needs its suffix).
pub fn is_defined(name: &str) -> bool {
    FLEET_VARS.iter().any(|v| match v.name.strip_suffix('_') {
        Some(_) => name.len() > v.name.len() && name.starts_with(v.name),
        None => v.name == name,
    })
}

/// The closest defined name, for a did-you-mean.
pub fn suggest(name: &str) -> Option<&'static str> {
    FLEET_VARS
        .iter()
        .map(|v| (crate::util::levenshtein_distance(name, v.name), v.name))
        .filter(|(d, v)| *d <= v.len().max(name.len()) / 3)
        .min_by_key(|(d, _)| *d)
        .map(|(_, v)| v)
}

/// Every `$FLEET_VAR_NAME` / `${FLEET_VAR_NAME}` in `text`, as (1-based line,
/// name without the prefix) — the same forms Fleet's regex finds.
pub fn references(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(at) = rest.find("FLEET_VAR_") {
            let before = &rest[..at];
            let after = &rest[at + "FLEET_VAR_".len()..];
            rest = after;
            if !(before.ends_with('$') || before.ends_with("${")) {
                continue;
            }
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.push((i + 1, name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> serde_json::Value {
        serde_json::from_str(include_str!("../data/fleet-gitops-keys.json")).unwrap()
    }

    fn sorted(v: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = v.iter().map(|s| s.to_string()).collect();
        v.sort();
        v
    }

    fn fleet_list(snap: &serde_json::Value, key: &str) -> Vec<String> {
        let mut v: Vec<String> = snap[key]
            .as_array()
            .unwrap_or_else(|| panic!("{key} missing from the snapshot"))
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn defines_exactly_fleets_variables() {
        let snap = snapshot();
        let ours: Vec<&str> = FLEET_VARS.iter().map(|v| v.name).collect();
        assert_eq!(
            sorted(&ours),
            fleet_list(&snap, "fleet_vars"),
            "FLEET_VARS drifted from Fleet {} (regenerate the snapshot, then update fleet_vars.rs)",
            snap["fleet_version"]
        );
    }

    #[test]
    fn allow_lists_match_fleets() {
        let snap = snapshot();
        let ctx = &snap["fleet_var_contexts"];
        for (go_name, ours) in [
            ("fleetVarsSupportedInAppleConfigProfiles", APPLE_PROFILE),
            ("fleetVarsSupportedInWindowsProfiles", WINDOWS_PROFILE),
            ("fleetVarsSupportedInDDMDeclarations", &IDP_AND_HOST[..]),
            ("FleetVarsSupportedInScripts", &IDP_AND_HOST[..]),
        ] {
            assert_eq!(sorted(ours), fleet_list(ctx, go_name), "{go_name}");
        }
        for p in APPLE_PROFILE_PREFIXES.iter().chain(WINDOWS_PROFILE_PREFIXES) {
            assert!(FLEET_VARS.iter().any(|v| v.name == *p), "{p} is not a Fleet prefix variable");
        }
    }

    #[test]
    fn contexts_and_references() {
        assert!(VarContext::AppleProfile.accepts("DIGICERT_DATA_MyCA"));
        assert!(!VarContext::AppleProfile.accepts("DIGICERT_DATA_"));
        assert!(!VarContext::WindowsProfile.accepts("DIGICERT_DATA_MyCA"));
        assert!(!VarContext::Script.accepts("NDES_SCEP_CHALLENGE"));
        assert!(!VarContext::AppleProfile.accepts("HOST_TARGET_OS_VERSION"));
        assert!(is_defined("SMALLSTEP_SCEP_CHALLENGE_ca1") && !is_defined("HOST_SERIAL"));
        assert_eq!(suggest("HOST_SERIAL_NUMBER"), None);
        assert_eq!(suggest("HOST_HARDWARE_SERIL"), Some("HOST_HARDWARE_SERIAL"));
        assert_eq!(
            references("a $FLEET_VAR_HOST_UUID b\n${FLEET_VAR_X_Y}z FLEET_VAR_NOPE"),
            vec![(1, "HOST_UUID".to_string()), (2, "X_Y".to_string())]
        );
    }
}
