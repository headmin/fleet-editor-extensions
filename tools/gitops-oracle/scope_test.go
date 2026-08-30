package main

import (
	"os"
	"path/filepath"
	"testing"
)

// The oracle's scoping must stay behaviorally identical to flint's
// FleetLintConfig::is_out_of_scope_file. If they drift, every difference
// shows up in a diff as a phantom "missing rule" or "false positive" — the
// exact failure this file exists to prevent (the first oracle run reported 10
// phantom gaps, all of them scope mismatch).
func TestScopeMatchesFlintSemantics(t *testing.T) {
	dir := t.TempDir()
	cfg := `
[files]
include = ["default.yml", "fleets/**", "labels/**", "platforms/**"]
exclude = ["platforms/_retired/**", "tools-scripts/**", "santa/**"]
`
	if err := os.WriteFile(filepath.Join(dir, ".fleetlint.toml"), []byte(cfg), 0o600); err != nil {
		t.Fatal(err)
	}
	scope, err := LoadScope(dir)
	if err != nil {
		t.Fatal(err)
	}

	for _, tc := range []struct {
		path string
		out  bool
		why  string
	}{
		{"default.yml", false, "exact filename in include"},
		{"fleets/ABC-GAMMA.yml", false, "include glob, one level"},
		{"platforms/macos/site/santa/x.mobileconfig", false,
			"anchored exclude 'santa/**' must NOT reach a nested santa dir"},
		{"platforms/macos/brand/XX/software/a.yml", false, "include glob, deep"},
		{"tools-scripts/precommit/x.yml", true, "excluded tree"},
		{"tools-scripts/ddm-examples/app.json", true, "exclude beats nothing-else"},
		{"platforms/_retired/old.yml", true, "exclude wins over include"},
		{"santa/.DS_Store", true, "top-level excluded dir"},
		{"README.md", true, "non-empty include is authoritative: omission = out"},
		{"resources/images/x.svg", true, "not listed in this config's include"},
	} {
		if got := scope.OutOfScope(filepath.Join(dir, tc.path)); got != tc.out {
			t.Errorf("OutOfScope(%q) = %v, want %v — %s", tc.path, got, tc.out, tc.why)
		}
	}
}

// With no include list, everything survives except explicit excludes —
// flint's denylist style.
func TestScopeDenylistOnly(t *testing.T) {
	dir := t.TempDir()
	cfg := "[files]\nexclude = [\"tools-scripts/**\"]\n"
	if err := os.WriteFile(filepath.Join(dir, ".fleetlint.toml"), []byte(cfg), 0o600); err != nil {
		t.Fatal(err)
	}
	scope, err := LoadScope(dir)
	if err != nil {
		t.Fatal(err)
	}
	if scope.OutOfScope(filepath.Join(dir, "anything/at/all.yml")) {
		t.Error("empty include must not exclude anything")
	}
	if !scope.OutOfScope(filepath.Join(dir, "tools-scripts/x.yml")) {
		t.Error("exclude must still apply")
	}
}

// No config found: nothing is out of scope, matching flint's no-config
// behavior. A missing config must never silently narrow the audit.
func TestScopeNoConfig(t *testing.T) {
	scope, err := LoadScope(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if scope.OutOfScope("/tmp/whatever/x.yml") {
		t.Error("absent config must leave everything in scope")
	}
}
