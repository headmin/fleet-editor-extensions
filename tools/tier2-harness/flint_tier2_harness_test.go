package fleetctl

// flint Tier 2 harness — runs the REAL `fleetctl gitops --dry-run` against a
// mocked datastore, so flint's rules can be checked against the actual
// command rather than against a reading of the source.
//
// This file does not live in flint's build. It is copied INTO Fleet's own
// package tree by ../run.sh, because the mock setup it needs
// (setupEmptyGitOpsMocks) is defined in a _test.go file and is therefore
// unimportable from anywhere else. That coupling is the whole reason for the
// copy-in approach — see ../README.md.
//
// Why Tier 2 exists at all: Tier 1 (tools/gitops-oracle) calls
// spec.GitOpsFromFile, which is only the parsing half. Everything fleetctl
// does AFTER parsing — premium gating, label resolution, fleet ordering,
// secret handling, VerifyLabelScopes — lives in the command layer and is
// invisible to Tier 1. Running the real command is the only way to reach it.
//
// No MySQL, no Redis, no Docker: RunServerWithMockedDS with
// redistest.NopRedis() stands up a real Fleet API over httptest in ~0.25s.

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/fleetdm/fleet/v4/cmd/fleetctl/fleetctl/testing_utils"
	"github.com/fleetdm/fleet/v4/server/datastore/redis/redistest"
	"github.com/fleetdm/fleet/v4/server/fleet"
	"github.com/fleetdm/fleet/v4/server/service"
	"github.com/stretchr/testify/require"
)

// snapshotLabels reads label names from a .fleet-snapshot.json.
//
// The snapshot is flint's answer to server state the repo cannot supply
// (crates/flint-lint/src/snapshot.rs): `flint fleet snapshot` records the
// label names the server knows, and the file is committed. Consuming the same
// file here means the harness and flint agree on which labels exist, instead
// of the harness asserting against a hand-maintained list that silently drifts
// from both.
//
// Looked up at $FLINT_SNAPSHOT, else .fleet-snapshot.json inside
// $FLINT_TIER2_REPO. Absent is fine — callers pass explicit labels for the
// hermetic contract tests.
func snapshotPath() string {
	if p := os.Getenv("FLINT_SNAPSHOT"); p != "" {
		return p
	}
	if repo := os.Getenv("FLINT_TIER2_REPO"); repo != "" {
		return filepath.Join(repo, ".fleet-snapshot.json")
	}
	return ""
}

// snapshotVPPLocations reads VPP organization units from the snapshot.
//
// validateVPPAssignments (appconfig.go:2179) matches every
// org_settings.mdm.volume_purchasing_program[].location against the tokens
// the server holds, and rejects an unknown one with "token with organization
// unit X doesn't exist". That is server state the repo cannot supply — the
// same class as label existence — so it comes from the same snapshot.
//
// Returning nil is correct when absent: the harness then has no tokens, which
// makes it STRICTER than the real server, never more permissive.
func snapshotVPPLocations(t *testing.T) []string {
	t.Helper()
	path := snapshotPath()
	if path == "" {
		return nil
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	var snap struct {
		Capabilities struct {
			VPPLocations []string `json:"vpp_locations"`
			MDMEnabled   bool     `json:"mdm_enabled"`
		} `json:"capabilities"`
	}
	if err := json.Unmarshal(raw, &snap); err != nil {
		t.Fatalf("parsing %s: %v", path, err)
	}
	if n := len(snap.Capabilities.VPPLocations); n > 0 {
		t.Logf("snapshot: %d VPP location(s)", n)
	}
	return snap.Capabilities.VPPLocations
}

// snapshotMDMEnabled reports capabilities.mdm_enabled.
func snapshotMDMEnabled(t *testing.T) bool {
	t.Helper()
	path := snapshotPath()
	if path == "" {
		return false
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		return false
	}
	var snap struct {
		Capabilities struct {
			MDMEnabled bool `json:"mdm_enabled"`
		} `json:"capabilities"`
	}
	if err := json.Unmarshal(raw, &snap); err != nil {
		t.Fatalf("parsing %s: %v", path, err)
	}
	return snap.Capabilities.MDMEnabled
}

func snapshotLabels(t *testing.T) []string {
	t.Helper()

	path := os.Getenv("FLINT_SNAPSHOT")
	if path == "" {
		if repo := os.Getenv("FLINT_TIER2_REPO"); repo != "" {
			path = filepath.Join(repo, ".fleet-snapshot.json")
		}
	}
	if path == "" {
		return nil
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Logf("no snapshot at %s (%v) — falling back to explicit labels", path, err)
		return nil
	}

	var snap struct {
		Schema uint `json:"schema"`
		Labels struct {
			Builtin []string `json:"builtin"`
			Custom  []string `json:"custom"`
		} `json:"labels"`
		Provenance struct {
			FetchedAt string `json:"fetched_at"`
		} `json:"provenance"`
	}
	if err := json.Unmarshal(raw, &snap); err != nil {
		// A malformed snapshot must fail loudly, not silently degrade into
		// "no labels" — that would surface as a pile of Unknown-label errors
		// with no hint at the real cause.
		t.Fatalf("parsing %s: %v", path, err)
	}

	names := append(append([]string{}, snap.Labels.Builtin...), snap.Labels.Custom...)
	t.Logf("snapshot %s: %d label(s), fetched %s",
		path, len(names), snap.Provenance.FetchedAt)
	return names
}

// newHarness boots a real Fleet API backed by a mock store.
//
// Premium is required or the reference repos fail to parse — labels_exclude_*
// are premium-gated (gitops.go:421-425). NopRedis sidesteps the REDIS_TEST
// skip; gitops parsing never touches Redis.
//
// Known labels are the UNION of any snapshot's labels and the explicit ones
// passed here. The union keeps the contract tests hermetic — they must pass
// with no snapshot present — while letting a real-repo run resolve against
// the actual server label set.
func newHarness(t *testing.T, knownLabels ...string) {
	t.Helper()
	knownLabels = append(knownLabels, snapshotLabels(t)...)
	license := &fleet.LicenseInfo{Tier: fleet.TierPremium, Expiration: time.Now().Add(24 * time.Hour)}
	_, ds := testing_utils.RunServerWithMockedDS(t, &service.TestServerOpts{
		License: license,
		Pool:    redistest.NopRedis(),
	})
	setupEmptyGitOpsMocks(ds)

	// Declare which labels the "server" knows. Deduped so a label present in
	// both the snapshot and the explicit list is not returned twice.
	//
	// This is Tier 2 reaching past Tier 1: fleetctl resolves every referenced
	// label against the server and rejects unknown ones
	// (gitops.go:481). spec.GitOpsFromFile cannot see that at all — it has no
	// server. Leaving this unmocked makes any fixture that references a label
	// fail for the WRONG reason, masking whatever the case was testing.
	// Must override AFTER setupEmptyGitOpsMocks, which stubs this to nil.
	// `fleetctl` reads known labels via GET /api/latest/fleet/spec/labels
	// (client_labels.go:42 GetLabels -> GetLabelSpecs), not the summary
	// endpoint.
	ds.GetLabelSpecsFunc = func(ctx context.Context, filter fleet.TeamFilter) ([]*fleet.LabelSpec, error) {
		out := make([]*fleet.LabelSpec, 0, len(knownLabels))
		seen := make(map[string]bool, len(knownLabels))
		for i, name := range knownLabels {
			if seen[name] {
				continue
			}
			seen[name] = true
			out = append(out, &fleet.LabelSpec{
				ID:                  uint(i + 1),
				Name:                name,
				Description:         "declared by the flint Tier 2 harness",
				LabelMembershipType: fleet.LabelMembershipTypeManual,
			})
		}
		return out, nil
	}

	// Mocks reached only by richer real-world configs.
	//
	// mock.Store panics on any unset Func (datastore_mock.go calls the field
	// unguarded), and a panic inside an HTTP handler surfaces at the client
	// as a bare EOF — so a missing mock looks like a network fault rather
	// than a missing stub. Each one below was found by running a real repo
	// through and reading the server-side stack trace.
	//
	// They return EMPTY rather than representative data on purpose: this
	// harness checks that a config is ACCEPTED, and inventing VPP tokens or
	// teams would let it accept configs a real server would reject.
	ds.TeamsSummaryFunc = func(ctx context.Context) ([]*fleet.TeamSummary, error) {
		// validateVPPAssignments (appconfig.go:2144) walks this.
		return nil, nil
	}
	// VPP tokens come from the snapshot when it has them. Without it this
	// stays empty, so a repo declaring a `location:` fails — which is the
	// honest answer: nothing here has verified that the token exists.
	vppLocations := snapshotVPPLocations(t)

	// Apple MDM state is server config, not repo content. Fleet refuses every
	// profile and setup-experience key without it ("macOS MDM isn't turned
	// on"), so a harness left at the mock default rejects configs a real
	// server accepts. Sourced from the snapshot; default false keeps the
	// contract tests hermetic.
	if snapshotMDMEnabled(t) {
		ds.AppConfigFunc = func(ctx context.Context) (*fleet.AppConfig, error) {
			cfg := &fleet.AppConfig{}
			cfg.MDM.EnabledAndConfigured = true
			cfg.MDM.AppleBMEnabledAndConfigured = true
			return cfg, nil
		}
	}
	ds.ListVPPTokensFunc = func(ctx context.Context) ([]*fleet.VPPTokenDB, error) {
		out := make([]*fleet.VPPTokenDB, 0, len(vppLocations))
		for i, loc := range vppLocations {
			out = append(out, &fleet.VPPTokenDB{
				ID:       uint(i + 1),
				OrgName:  loc,
				Location: loc,
				Teams:    nil, // nil = "All teams", so team scoping never blocks
			})
		}
		return out, nil
	}

	t.Setenv("FLEET_SERVER_URL", "https://fleet.example.com")
	t.Setenv("ORG_NAME", "flint Tier 2 harness")
}

// globalYAMLWith returns a minimal valid global config with `extra` appended.
func globalYAMLWith(extra string) string {
	return `
controls:
policies:
agent_options:
org_settings:
  server_settings:
    server_url: $FLEET_SERVER_URL
  org_info:
    contact_url: https://example.com/contact
    org_name: ${ORG_NAME}
  secrets:
` + extra
}

func writeYAML(t *testing.T, dir, name, body string) string {
	t.Helper()
	p := filepath.Join(dir, name)
	require.NoError(t, os.WriteFile(p, []byte(body), 0o600))
	return p
}

// dryRun runs the genuine command and returns (output, error).
func dryRun(t *testing.T, path string) (string, error) {
	t.Helper()
	out, err := runAppNoChecks([]string{"gitops", "-f", path, "--dry-run"})
	s := ""
	if out != nil {
		s = out.String()
	}
	t.Logf("gitops --dry-run %s\n%s", filepath.Base(path), s)
	return s, err
}

// TestTier2SeverityContracts pins the severity decisions flint's rules encode.
//
// Each case is a claim flint makes about what Fleet does. A rule that reports
// an ERROR asserts "the apply fails"; a rule that reports a WARNING asserts
// "the apply succeeds but something is off". Getting that backwards is not a
// cosmetic issue — an over-strict rule blocks commits on valid config, which
// is how four false positives shipped before this harness existed.
func TestTier2SeverityContracts(t *testing.T) {
	newHarness(t, "Eng", "QA")

	dir := t.TempDir()
	empty := filepath.Join(dir, "empty")
	require.NoError(t, os.MkdirAll(empty, 0o755))

	t.Run("zero-match glob does NOT fail", func(t *testing.T) {
		// flint: broken-reference, WARNING.
		// Fleet: expandBaseItems logs "[!] ... matched no ... files" and
		// continues without recording an error. All entity types route
		// through that one function. Verified in production: a repo with
		// this glob active in 24 fleets applied successfully.
		p := writeYAML(t, dir, "globzero.yml", globalYAMLWith(
			"reports:\n  - paths: ./empty/*.yml\n"))
		out, err := dryRun(t, p)
		require.NoError(t, err, "a zero-match glob must not fail the apply")
		require.Contains(t, out, "matched no report files",
			"Fleet should still ADVISE about it — if this line disappears, "+
				"flint's warning has no counterpart and should be revisited")
	})

	t.Run("missing path: target DOES fail", func(t *testing.T) {
		// The control. Without it the case above proves nothing: a harness
		// that never fails cannot distinguish "Fleet accepts this" from
		// "this harness is broken".
		p := writeYAML(t, dir, "missingpath.yml", globalYAMLWith(
			"queries:\ncontrols:\n  macos_settings:\n    custom_settings:\n"+
				"      - path: ./empty/does-not-exist.mobileconfig\n"))
		_, err := dryRun(t, p)
		require.Error(t, err, "a missing path: target MUST fail — "+
			"if this passes, every other assertion here is meaningless")
	})

	t.Run("policy label overlap DOES fail", func(t *testing.T) {
		// flint: label-targeting, ERROR, policies only.
		// Fleet: verifyPolicyLabelScopes -> LabelOverlap. This check lives in
		// the COMMAND layer (getLabelUsage), so Tier 1 cannot see it — this
		// case is precisely why Tier 2 exists.
		p := writeYAML(t, dir, "overlap.yml",
			"name: Harness Fleet\nqueries:\nagent_options:\ncontrols:\n"+
				"team_settings:\n  secrets:\n"+
				"policies:\n  - name: Overlap\n    query: SELECT 1\n    platform: darwin\n"+
				"    labels_include_any:\n      - Eng\n"+
				"    labels_exclude_any:\n      - Eng\n")
		_, err := dryRun(t, p)
		require.Error(t, err, "the same label on both sides must fail")
		require.Contains(t, strings.ToLower(err.Error()), "include and an exclude",
			"expected Fleet's LabelOverlap rejection")
	})

	t.Run("empty label list is NOT a set value", func(t *testing.T) {
		// flint: label-targeting measures presence BY VALUE, not by key.
		// Fleet counts len(slice) > 0 everywhere, and policies.go:203 names
		// this exact shape as valid.
		p := writeYAML(t, dir, "emptylist.yml",
			"name: Harness Fleet\nqueries:\nagent_options:\ncontrols:\n"+
				"team_settings:\n  secrets:\n"+
				"policies:\n  - name: Empty include\n    query: SELECT 1\n    platform: darwin\n"+
				"    labels_include_any: []\n"+
				"    labels_include_all:\n      - Eng\n")
		_, err := dryRun(t, p)
		require.NoError(t, err, "an empty list must count as unset")
	})
}

// TestTier2Repo runs the real command over a real GitOps repo when
// FLINT_TIER2_REPO points at one. Skipped otherwise so the harness stays
// useful in CI without checking a fixture repo in.
//
// Files are passed the way the GitHub Action does it: the global config
// first, then every fleet file.
func TestTier2Repo(t *testing.T) {
	repo := os.Getenv("FLINT_TIER2_REPO")
	if repo == "" {
		t.Skip("set FLINT_TIER2_REPO=/path/to/gitops-repo to run against a real repo")
	}
	// No explicit labels: a real repo's labels must come from its snapshot.
	// If it has none, every label reference fails as Unknown — which is the
	// honest answer, since without server knowledge nothing here is verified.
	newHarness(t)

	args := []string{"gitops"}
	global := filepath.Join(repo, "default.yml")
	if _, err := os.Stat(global); err == nil {
		args = append(args, "-f", global)
	}
	fleets, _ := filepath.Glob(filepath.Join(repo, "fleets", "*.yml"))
	for _, f := range fleets {
		args = append(args, "-f", f)
	}
	require.Greater(t, len(args), 1, "no config files found under %s", repo)
	args = append(args, "--dry-run")

	out, err := runAppNoChecks(args)
	if out != nil {
		t.Logf("gitops --dry-run over %s (%d file(s))\n%s", repo, len(fleets)+1, out.String())
	}
	require.NoError(t, err, "the reference repo must pass a real dry-run")
}
