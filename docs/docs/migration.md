---
icon: lucide/arrow-right-left
---

# Migration

Flint helps migrate Fleet GitOps repos between versions. The `flint migrate` command generates a JSON report — it does **not** apply changes directly. An AI agent or script uses the report to execute the migration.

## Generate a migration report

```bash
flint migrate /path/to/gitops-repo --target-version 4.85.0
```

Output:

```json
{
  "version": "0.1.1",
  "target_version": "4.85.0",
  "summary": {
    "files_scanned": 121,
    "directory_renames": 1,
    "file_renames": 0,
    "key_renames": 14,
    "safe_fixes": 14
  },
  "directory_renames": [
    { "old": "teams", "new": "fleets", "files_affected": 7 }
  ],
  "file_renames": [],
  "file_changes": [
    {
      "path": "teams/engineering.yml",
      "move_to": "fleets/engineering.yml",
      "key_renames": [
        { "line": 2, "old_key": "team_settings", "new_key": "settings", "safety": "safe" },
        { "line": 5, "old_key": "queries", "new_key": "reports", "safety": "safe" }
      ]
    }
  ]
}
```

## Current renames

These renames were introduced as warnings in Fleet v4.80.1:

| Type | Old | New |
|---|---|---|
| Directory | `teams/` | `fleets/` |
| File | `no-team.yml` | `unassigned.yml` |
| Key | `team_settings` | `settings` |
| Key | `queries` | `reports` |

## Migration steps

1. **Directory renames first** — `mv teams/ fleets/`
2. **File renames** — `mv no-team.yml unassigned.yml` (also inside moved directories)
3. **Key renames** — apply bottom-up by line number to preserve offsets
4. **Path references** — update `path:` values referencing old directories
5. **Verify** — `flint check .` should show zero deprecation warnings

## Agent-assisted migration

Flint includes a Claude Code skill for agent-driven migration:

```bash
flint setup-agent    # Installs fleet-migrate skill
```

Then ask your agent: "Migrate my Fleet GitOps repo to 4.85." The agent runs `flint migrate`, interprets the JSON report, and applies changes step by step.

## Directory layout: use `platforms/`

`platforms/` is the standard Fleet GitOps layout — the structure `fleetctl new`
generates and what flint targets. The older `lib/` convention is **deprecated
legacy**: flint still lints `lib/` repos so existing setups keep working, but
new and migrated repos should use `platforms/`. If your repo has no `lib/`
directory, you're already on the standard layout — nothing to do here.

### v4.83 directory structure

```
it-and-security/
├── default.yml
├── fleets/
│   ├── workstations.yml
│   └── unassigned.yml
├── labels/
│   └── *.yml
└── platforms/
    ├── all/
    │   ├── icons/
    │   ├── policies/
    │   └── reports/
    ├── macos/
    │   ├── configuration-profiles/    # .mobileconfig
    │   ├── declaration-profiles/      # .json DDM
    │   ├── commands/
    │   ├── enrollment-profiles/
    │   ├── policies/
    │   ├── reports/
    │   ├── scripts/
    │   └── software/
    ├── windows/
    │   ├── configuration-profiles/    # .xml CSP
    │   ├── policies/
    │   ├── reports/
    │   ├── scripts/
    │   └── software/
    ├── linux/
    │   ├── policies/
    │   ├── reports/
    │   ├── scripts/
    │   └── software/
    ├── ios/
    │   ├── configuration-profiles/
    │   └── declaration-profiles/
    ├── ipados/
    │   ├── configuration-profiles/
    │   └── declaration-profiles/
    └── android/
        ├── configuration-profiles/
        └── managed-app-configurations/
```

### Key changes from legacy / v4.82

#### Directory mapping

If you're still on `lib/`, move to the standard `platforms/` layout:

| Deprecated (`lib/`) | Standard (`platforms/`) |
|---|---|
| `lib/macos/configuration-profiles/` | `platforms/macos/configuration-profiles/` |
| `lib/macos/scripts/` | `platforms/macos/scripts/` |
| `lib/all/labels/` | `labels/` |
| `lib/agent-options.yml` | inline in fleet YAML or `platforms/all/agent-options.yml` |

#### YAML key changes

| Old | New | Notes |
|---|---|---|
| `macos_settings` | `apple_settings` | v4.83 rename |
| `custom_settings` | `configuration_profiles` | v4.83 rename |
| `team_settings` | `settings` | v4.82 rename |
| `queries` | `reports` | v4.82 rename |

#### Profile references: per-file to glob patterns

```yaml
# v4.82 (per-file path:)
controls:
  macos_settings:
    custom_settings:
      - path: ../platforms/macos/configuration-profiles/wifi.mobileconfig
      - path: ../platforms/macos/configuration-profiles/vpn.mobileconfig

# v4.83 (glob paths:)
controls:
  apple_settings:
    configuration_profiles:
      - paths: ../platforms/macos/declaration-profiles/*.json
      - paths: ../platforms/macos/configuration-profiles/*.mobileconfig
  windows_settings:
    configuration_profiles:
      - paths: ../platforms/windows/configuration-profiles/*.xml
  scripts:
    - paths: ../platforms/macos/scripts/*.sh
    - paths: ../platforms/windows/scripts/*.ps1
    - paths: ../platforms/linux/scripts/*.sh
```

#### Profile identity (PayloadUUID)

!!! note
    Apple MDM identifies a configuration profile by the top-level `PayloadUUID` inside the `.mobileconfig`, not by its filename or path. As long as the UUID is unchanged, Fleet MDM (and most other MDM platforms) recognize the profile across edits and will not redeploy it — file renames, directory moves, and YAML reference restructuring don't trigger a redeploy on their own. Changing the UUID does.

#### Labels: per-file to glob

```yaml
# v4.82
labels:
  - path: ./labels/my-label.yml

# v4.83
labels:
  - paths: ./labels/*.yml
```

### Migration steps (legacy/v4.82 to v4.83)

```bash
# 1. Create new directory structure
mkdir -p platforms/{all/{icons,policies,reports},macos/{commands,configuration-profiles,declaration-profiles,enrollment-profiles,policies,reports,scripts,software},windows/{configuration-profiles,policies,reports,scripts,software},linux/{policies,reports,scripts,software},ios/{configuration-profiles,declaration-profiles},ipados/{configuration-profiles,declaration-profiles},android/{configuration-profiles,managed-app-configurations}}
mkdir -p labels fleets

# 2. Move files from lib/ to platforms/
mv lib/macos/configuration-profiles/*.mobileconfig platforms/macos/configuration-profiles/
mv lib/macos/configuration-profiles/*.json platforms/macos/declaration-profiles/
mv lib/macos/scripts/* platforms/macos/scripts/
mv lib/all/labels/*.yml labels/

# 3. Update fleet YAML keys and paths
# macos_settings -> apple_settings
# custom_settings -> configuration_profiles
# path: per-file -> paths: glob patterns

# 4. Verify
flint check .
```

For the full step-by-step guide with CI/CD setup, see the [contour fleet-migrate SOP](https://github.com/headmin/contour) (`contour help-ai --sop fleet-migrate`).
