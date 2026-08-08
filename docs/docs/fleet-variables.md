---
icon: lucide/variable
---

# Fleet variables

Fleet supports dynamic variable substitution in configuration profiles, scripts, and other contexts. Variables use the `$FLEET_VAR_` prefix and are resolved at apply time by the Fleet server.

Flint's LSP provides completion for these variables in YAML files.

## Host variables

| Variable | Description |
|----------|-------------|
| `$FLEET_VAR_HOST_HARDWARE_SERIAL` | Host hardware serial number |
| `$FLEET_VAR_HOST_UUID` | Host UUID |
| `$FLEET_VAR_HOST_PLATFORM` | Host platform (`darwin`, `windows`, `linux`) |
| `$FLEET_VAR_HOST_END_USER_IDP_USERNAME` | End user IdP username |
| `$FLEET_VAR_HOST_END_USER_IDP_USERNAME_LOCAL_PART` | Local part of IdP username (before @) |
| `$FLEET_VAR_HOST_END_USER_IDP_FULL_NAME` | End user full name from IdP |
| `$FLEET_VAR_HOST_END_USER_IDP_GROUPS` | End user IdP groups |
| `$FLEET_VAR_HOST_END_USER_IDP_DEPARTMENT` | End user IdP department |
| `$FLEET_VAR_HOST_END_USER_EMAIL_IDP` | End user email from IdP *(legacy, avoid in new configs)* |

## Certificate variables

| Variable | Description |
|----------|-------------|
| `$FLEET_VAR_NDES_SCEP_CHALLENGE` | NDES SCEP challenge value |
| `$FLEET_VAR_NDES_SCEP_PROXY_URL` | NDES SCEP proxy URL |
| `$FLEET_VAR_SCEP_RENEWAL_ID` | SCEP certificate renewal ID |
| `$FLEET_VAR_SCEP_WINDOWS_CERTIFICATE_ID` | Windows SCEP certificate ID |

## Certificate authority variables (prefix)

These require a suffix — append the CA name (e.g., `$FLEET_VAR_DIGICERT_DATA_MyCA`):

| Variable prefix | Description |
|-----------------|-------------|
| `$FLEET_VAR_DIGICERT_DATA_` | DigiCert certificate data for specified CA |
| `$FLEET_VAR_DIGICERT_PASSWORD_` | DigiCert password for specified CA |
| `$FLEET_VAR_CUSTOM_SCEP_CHALLENGE_` | Custom SCEP challenge for specified CA |
| `$FLEET_VAR_CUSTOM_SCEP_PROXY_URL_` | Custom SCEP proxy URL for specified CA |
| `$FLEET_VAR_SMALLSTEP_SCEP_CHALLENGE_` | Smallstep SCEP challenge for specified CA |
| `$FLEET_VAR_SMALLSTEP_SCEP_PROXY_URL_` | Smallstep SCEP proxy URL for specified CA |

## Usage in profiles

```xml
<!-- In .mobileconfig profiles -->
<string>$FLEET_VAR_HOST_END_USER_IDP_USERNAME</string>
```

```yaml
# In Fleet GitOps YAML
controls:
  apple_settings:
    configuration_profiles:
      - path: ../platforms/macos/configuration-profiles/wifi.mobileconfig
```

Fleet resolves `$FLEET_VAR_*` placeholders when the profile is delivered to the host.

!!! warning
    Variables are resolved server-side at delivery time. `flint check` does **not** validate variable values — it only checks that the YAML structure is correct.

## References

- [Fleet documentation: GitOps YAML files](https://fleetdm.com/docs/configuration/yaml-files)
- [Fleet source: server/fleet/mdm.go](https://github.com/fleetdm/fleet/blob/main/server/fleet/mdm.go)
