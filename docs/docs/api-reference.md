---
icon: lucide/globe
---

# Fleet API reference

The Fleet REST endpoints flint uses, plus the ones you'll most often want next
to it. Checked against Fleet v4.93.0's router. All require
`Authorization: Bearer <token>`.

Base URL: `<fleet-url>/api/v1/fleet/`

!!! note "Where flint talks to Fleet"
    - **`flint fleet …`** and **`flint dry-run --refresh-snapshot`** call the
      REST API directly, read-only (GET only).
    - **The editor** (`gitops_validation`, `live_completions`) shells out to
      `fleetctl` instead; see [fleetctl equivalents](#fleetctl-equivalents).

!!! note "`/teams` and `/queries` paths"
    Since Fleet 4.83 the canonical paths are `/fleets` and `/reports` (and
    `/ab_tokens` for Apple Business). The old `/teams`, `/queries` and
    `/abm_tokens` paths still work as deprecated aliases (Fleet logs a
    deprecation warning). flint calls the old paths so it also works against
    older Fleet servers.

## GitOps

There is no single GitOps endpoint. `fleetctl gitops` parses your YAML locally
and applies it through the spec endpoints, each with `?dry_run=true` for
`--dry-run`:

| Endpoint | Method | Path |
|---|---|---|
| Apply fleets | `POST` | `/api/v1/fleet/spec/fleets` |
| Apply policies | `POST` | `/api/v1/fleet/spec/policies` |
| Apply reports | `POST` | `/api/v1/fleet/spec/reports` |
| Apply labels | `POST` | `/api/v1/fleet/spec/labels` |

## Fleets

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| List fleets | `GET` | `/api/v1/fleet/fleets` | `flint fleet teams`, snapshot (via the `/teams` alias) |
| Get fleet | `GET` | `/api/v1/fleet/fleets/:id` | — |
| Create fleet | `POST` | `/api/v1/fleet/fleets` | — |
| Modify fleet | `PATCH` | `/api/v1/fleet/fleets/:id` | — |
| Delete fleet | `DELETE` | `/api/v1/fleet/fleets/:id` | — |

## Reports

| Endpoint | Method | Path |
|---|---|---|
| List reports | `GET` | `/api/v1/fleet/reports` |
| Get report | `GET` | `/api/v1/fleet/reports/:id` |
| Create report | `POST` | `/api/v1/fleet/reports` |
| Delete report | `DELETE` | `/api/v1/fleet/reports/id/:id` (or `/reports/:name`) |
| Run live query | `POST` | `/api/v1/fleet/reports/run` |

## Labels

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| List labels | `GET` | `/api/v1/fleet/labels` | `flint fleet labels`, snapshot |
| Get label | `GET` | `/api/v1/fleet/labels/:id` | — |
| Create label | `POST` | `/api/v1/fleet/labels` | — |
| Delete label | `DELETE` | `/api/v1/fleet/labels/id/:id` (or `/labels/:name`) | — |

## Policies

| Endpoint | Method | Path |
|---|---|---|
| List policies | `GET` | `/api/v1/fleet/policies` |
| Get policy | `GET` | `/api/v1/fleet/policies/:id` |
| Create policy | `POST` | `/api/v1/fleet/policies` |
| Delete policies | `POST` | `/api/v1/fleet/policies/delete` (body: `{"ids": [...]}`) |
| Fleet policies | `GET` | `/api/v1/fleet/fleets/:id/policies` |

## Software

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| List software | `GET` | `/api/v1/fleet/software/titles` | `flint fleet software`, snapshot |
| Get software | `GET` | `/api/v1/fleet/software/titles/:id` | — |
| Upload package | `POST` | `/api/v1/fleet/software/package` | — |
| List FMA | `GET` | `/api/v1/fleet/software/fleet_maintained_apps` | `flint fleet fma`, snapshot |

## Configuration

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| Get config | `GET` | `/api/v1/fleet/config` | snapshot |
| Modify config | `PATCH` | `/api/v1/fleet/config` | — |
| Version | `GET` | `/api/v1/fleet/version` | `flint fleet status`, `flint fleet doctor`, snapshot |
| VPP tokens | `GET` | `/api/latest/fleet/vpp_tokens` | snapshot |
| Apple Business tokens | `GET` | `/api/latest/fleet/ab_tokens` | snapshot (via the `/abm_tokens` alias) |

## Hosts

| Endpoint | Method | Path |
|---|---|---|
| List hosts | `GET` | `/api/v1/fleet/hosts` |
| Get host | `GET` | `/api/v1/fleet/hosts/:id` |
| Get host by identifier | `GET` | `/api/v1/fleet/hosts/identifier/:identifier` |
| Delete host | `DELETE` | `/api/v1/fleet/hosts/:id` |

## Authentication

```bash
# Get token
curl -X POST https://fleet.example.com/api/v1/fleet/login \
  -d '{"email":"admin@example.com","password":"..."}' \
  -H 'Content-Type: application/json'

# Use token
curl -H "Authorization: Bearer $FLEET_API_TOKEN" \
  https://fleet.example.com/api/v1/fleet/hosts?per_page=10
```

## fleetctl equivalents

The editor features shell out to `fleetctl`; these are the exact commands.

| `.fleetlint.toml` `[fleet]` setting | fleetctl command |
|---|---|
| `gitops_validation = true` | `fleetctl gitops --dry-run -f <file>` (on save) |
| `live_completions = true` | `fleetctl get labels --yaml` / `fleetctl get teams --yaml` / `fleetctl get queries --yaml` |

`teams` and `queries` are aliases fleetctl keeps for `fleets` and `reports`
(Fleet 4.82+), so the same commands work with older and newer fleetctl.

## Full API documentation

[fleetdm.com/docs/rest-api/rest-api](https://fleetdm.com/docs/rest-api/rest-api)
