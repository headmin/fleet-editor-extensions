---
icon: lucide/variable
---

# Fleet variables

Fleet replaces `$FLEET_VAR_*` (or `${FLEET_VAR_*}`) placeholders per host when
it delivers a profile or runs a script. It also **validates them on upload**: a
variable Fleet doesn't know, or one not allowed in that kind of file, fails the
apply — for example "Fleet variable $FLEET_VAR_X is not supported in
configuration profiles."

## What flint does

- **`fleet-variable` (error):** every profile (`.mobileconfig`, DDM `.json`,
  Windows `.xml`), script and GitOps YAML file is checked against the list
  Fleet uses for that file type, with a did-you-mean for typos.
- **Completion:** typing `$FLEET_VAR_` in a GitOps YAML file lists the variables
  below.
- The lists are read from Fleet's source (`server/fleet`, `server/service`) at
  the Fleet version flint is pinned to (v4.93.0), and a test fails when they
  drift.

## Variables and where Fleet accepts them

| Variable | Description | Apple profiles | DDM | Windows profiles | Scripts |
|---|---|---|---|---|---|
| `$FLEET_VAR_HOST_HARDWARE_SERIAL` | Host hardware serial number | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_UUID` | Host UUID | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_PLATFORM` | Host platform (darwin, windows, linux, …) | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_END_USER_IDP_USERNAME` | End user's IdP username | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_END_USER_IDP_USERNAME_LOCAL_PART` | Local part of the IdP username (before @) | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_END_USER_IDP_FULL_NAME` | End user's full name from the IdP | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_END_USER_IDP_GROUPS` | End user's IdP groups | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_END_USER_IDP_DEPARTMENT` | End user's IdP department | ✓ | ✓ | ✓ | ✓ |
| `$FLEET_VAR_HOST_END_USER_EMAIL_IDP` | End user's email from the IdP (legacy; prefer HOST_END_USER_IDP_USERNAME) | ✓ |  |  |  |
| `$FLEET_VAR_NDES_SCEP_CHALLENGE` | NDES SCEP challenge | ✓ |  | ✓ |  |
| `$FLEET_VAR_NDES_SCEP_PROXY_URL` | NDES SCEP proxy URL | ✓ |  | ✓ |  |
| `$FLEET_VAR_CERTIFICATE_RENEWAL_ID` | Fleet-managed ID for automatic certificate renewal (put it in the certificate's OU) | ✓ |  | ✓ |  |
| `$FLEET_VAR_SCEP_RENEWAL_ID` | Legacy name of CERTIFICATE_RENEWAL_ID | ✓ |  | ✓ |  |
| `$FLEET_VAR_SCEP_WINDOWS_CERTIFICATE_ID` | Windows SCEP certificate ID |  |  | ✓ |  |
| `$FLEET_VAR_PSSO_DEVICE_REGISTRATION_TOKEN` | Per-device Platform SSO registration token (RegistrationToken key only; Fleet Premium) | ✓ |  |  |  |
| `$FLEET_VAR_DIGICERT_DATA_<CA>` | DigiCert certificate data for the named CA | ✓ |  |  |  |
| `$FLEET_VAR_DIGICERT_PASSWORD_<CA>` | DigiCert certificate password for the named CA | ✓ |  |  |  |
| `$FLEET_VAR_CUSTOM_SCEP_CHALLENGE_<CA>` | Custom SCEP challenge for the named CA | ✓ |  | ✓ |  |
| `$FLEET_VAR_CUSTOM_SCEP_PROXY_URL_<CA>` | Custom SCEP proxy URL for the named CA | ✓ |  | ✓ |  |
| `$FLEET_VAR_SMALLSTEP_SCEP_CHALLENGE_<CA>` | Smallstep SCEP challenge for the named CA | ✓ |  |  |  |
| `$FLEET_VAR_SMALLSTEP_SCEP_PROXY_URL_<CA>` | Smallstep SCEP proxy URL for the named CA | ✓ |  |  |  |
| `$FLEET_VAR_HOST_TARGET_OS_VERSION` | Fleet-internal (Fleet's own OS-update declaration); not for your files |  |  |  |  |
| `$FLEET_VAR_HOST_TARGET_OS_DEADLINE` | Fleet-internal (Fleet's own OS-update declaration); not for your files |  |  |  |  |
| `$FLEET_VAR_PATCH_NOTIFICATION_URL` | Fleet-internal (patch notifications); not for your files |  |  |  |  |

`<CA>` is the name of a certificate authority configured in Fleet, e.g.
`$FLEET_VAR_DIGICERT_DATA_MyCA`. In GitOps YAML, other lists apply (host name
templates, app configurations); flint only reports names Fleet doesn't define
there.

## Usage in profiles

```xml
<!-- In a .mobileconfig -->
<string>$FLEET_VAR_HOST_END_USER_IDP_USERNAME</string>
```

```yaml
# In Fleet GitOps YAML
controls:
  apple_settings:
    configuration_profiles:
      - path: ../platforms/macos/configuration-profiles/wifi.mobileconfig
```

!!! note
    flint checks that a variable exists and is allowed in that file type. It
    does not check runtime conditions Fleet applies on top — a configured
    certificate authority, a Fleet Premium license, or that
    `PSSO_DEVICE_REGISTRATION_TOKEN` sits in the `RegistrationToken` key.

## References

- [Fleet documentation: GitOps YAML files](https://fleetdm.com/docs/configuration/yaml-files)
- [Fleet source: server/fleet/mdm.go](https://github.com/fleetdm/fleet/blob/main/server/fleet/mdm.go)
