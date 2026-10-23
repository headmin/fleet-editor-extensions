---
icon: lucide/globe
---

# Fleet API reference

Key Fleet REST API endpoints used by flint's live completions and GitOps validation. All endpoints require `Authorization: Bearer <token>`.

Base URL: `<fleet-url>/api/v1/fleet/`

## GitOps

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| Apply GitOps | `POST` | `/api/v1/fleet/gitops` | `gitops_validation` (dry-run) |
| Generate GitOps | `GET` | `/api/v1/fleet/gitops/generate` | `fleetctl generate-gitops` |

## Fleets (teams)

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| List fleets | `GET` | `/api/v1/fleet/teams` | `live_completions` (fleet names) |
| Get fleet | `GET` | `/api/v1/fleet/teams/:id` | — |
| Create fleet | `POST` | `/api/v1/fleet/teams` | — |
| Modify fleet | `PATCH` | `/api/v1/fleet/teams/:id` | — |
| Delete fleet | `DELETE` | `/api/v1/fleet/teams/:id` | — |

!!! note
    The API still uses `teams` in endpoint paths. The `fleets` rename is YAML-side only (Fleet 4.82+).

## Reports (queries)

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| List reports | `GET` | `/api/v1/fleet/queries` | `live_completions` (report names) |
| Get report | `GET` | `/api/v1/fleet/queries/:id` | — |
| Create report | `POST` | `/api/v1/fleet/queries` | — |
| Run live query | `POST` | `/api/v1/fleet/queries/run` | — |

!!! note
    The API still uses `queries` in endpoint paths. The `reports` rename is YAML-side only (Fleet 4.82+).

## Labels

| Endpoint | Method | Path | Used by |
|---|---|---|---|
| List labels | `GET` | `/api/v1/fleet/labels` | `live_completions` (label names + details) |
| Get label | `GET` | `/api/v1/fleet/labels/:id` | — |
| Create label | `POST` | `/api/v1/fleet/labels` | — |
| Delete label | `DELETE` | `/api/v1/fleet/labels/:id` | — |

## Policies

| Endpoint | Method | Path |
|---|---|---|
| List policies | `GET` | `/api/v1/fleet/policies` |
| Get policy | `GET` | `/api/v1/fleet/policies/:id` |
| Create policy | `POST` | `/api/v1/fleet/policies` |
| Delete policy | `DELETE` | `/api/v1/fleet/policies/:id` |
| Fleet policies | `GET` | `/api/v1/fleet/teams/:id/policies` |

## Software

| Endpoint | Method | Path |
|---|---|---|
| List software | `GET` | `/api/v1/fleet/software/titles` |
| Get software | `GET` | `/api/v1/fleet/software/titles/:id` |
| Upload package | `POST` | `/api/v1/fleet/software/packages` |
| List FMA | `GET` | `/api/v1/fleet/software/fleet_maintained_apps` |

## Configuration

| Endpoint | Method | Path |
|---|---|---|
| Get config | `GET` | `/api/v1/fleet/config` |
| Modify config | `PATCH` | `/api/v1/fleet/config` |

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
