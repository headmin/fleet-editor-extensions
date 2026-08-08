// Command gitops-oracle reports what Fleet's OWN parser thinks of a set of
// GitOps YAML files.
//
// # Why this exists
//
// flint reimplements Fleet's GitOps validation in Rust. Any reimplementation
// drifts, and drift in this direction is expensive: a rule that claims "Fleet
// rejects this at apply time" when Fleet does not sends users to edit working
// configuration. Five such rules shipped before this tool existed
// (label-targeting, categories, fma-slug, glob-zero, and the org logo keys).
//
// This is the differential oracle. It calls spec.GitOpsFromFile — the exact
// function `fleetctl gitops` calls — and emits its findings as JSON so they
// can be diffed against `flint check --format json`. Every disagreement is
// either a flint false positive or a missing flint rule.
//
// # What it covers
//
// GitOpsFromFile is the pure, offline half of gitops validation: structure,
// unknown keys, deprecated key renames, env expansion, path/paths resolution,
// glob expansion, duplicate basenames, extension allowlists, policy ->
// script/software cross-references, and the software label rules.
//
// Policy label scoping is NOT in that call tree — `fleetctl` runs it from the
// command layer (cmd/fleetctl/fleetctl/gitops.go:1106, inside getLabelUsage,
// which is package-private). PolicySpec.VerifyLabelScopes is exported and
// pure, so this tool calls it directly to close that gap.
//
// What it cannot cover: anything needing live server state — whether a label
// exists, team IDs, VPP/ABM tokens, software title IDs, FLEET_SECRET_*
// presence. Those need the Tier 2 harness (a mocked Fleet API).
//
// Usage:
//
//	gitops-oracle [-premium] [-base DIR] FILE...
//	gitops-oracle -repo PATH            # every *.yml under PATH
package main

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/fleetdm/fleet/v4/pkg/spec"
	"github.com/fleetdm/fleet/v4/server/fleet"
	"github.com/hashicorp/go-multierror"
	"gopkg.in/yaml.v3"
)

// Finding is one thing Fleet's parser said about a file.
type Finding struct {
	// "error" (fails the apply) or "warning" (Fleet's [!] log lines).
	Severity string `json:"severity"`
	Message  string `json:"message"`
	// Where the finding came from, so a flint diff can attribute it:
	// "GitOpsFromFile" or "VerifyLabelScopes".
	Source string `json:"source"`
}

// FileResult is the oracle's verdict on a single YAML file.
type FileResult struct {
	Path   string `json:"path"`
	Parsed bool   `json:"parsed"`
	// NotEntryFile marks a YAML that Fleet's parser cannot judge on its own.
	// GitOpsFromFile only accepts a fleet file (`name:`) or the global config
	// (`org_settings:`); a profile, a standalone policy list, or any file
	// pulled in via `path:`/`paths:` is validated as part of its PARENT, never
	// alone. flint lints those files individually, so a naive per-file diff
	// would read Fleet's "no name provided" as a disagreement. It is not one —
	// it means "ask the parent instead", so these are excluded from the diff.
	NotEntryFile bool      `json:"not_entry_file,omitempty"`
	TeamName     *string   `json:"team_name,omitempty"`
	Findings     []Finding `json:"findings"`
}

// Report is the top-level JSON document.
type Report struct {
	// Fleet source revision this binary was built against. Recorded because
	// the oracle is only as current as the Fleet module it links.
	FleetModule string `json:"fleet_module"`
	Premium     bool   `json:"premium"`
	// Scoping actually applied, echoed so a report is self-describing: a
	// reader can tell whether a file is absent because Fleet was happy or
	// because it was never asked about.
	ScopeConfig       string       `json:"scope_config,omitempty"`
	ScopeInclude      []string     `json:"scope_include,omitempty"`
	ScopeExclude      []string     `json:"scope_exclude,omitempty"`
	SkippedOutOfScope int          `json:"skipped_out_of_scope"`
	Files             []FileResult `json:"files"`
	Summary           Summary      `json:"summary"`
}

type Summary struct {
	Files    int `json:"files"`
	Errors   int `json:"errors"`
	Warnings int `json:"warnings"`
}

func main() {
	var (
		premium = flag.Bool("premium", true, "treat the license as Fleet Premium (most GitOps repos rely on premium keys)")
		repo    = flag.String("repo", "", "scan every *.yml/*.yaml under this directory instead of listing files")
		base    = flag.String("base", "", "base directory for relative path resolution (default: each file's own directory, matching fleetctl)")
		pretty  = flag.Bool("pretty", true, "indent the JSON output")
		noScope = flag.Bool("no-scope", false, "ignore .fleetlint.toml [files] scoping and inspect every file")
	)
	flag.Parse()

	files, err := collectFiles(*repo, flag.Args())
	if err != nil {
		fmt.Fprintf(os.Stderr, "gitops-oracle: %v\n", err)
		os.Exit(2)
	}

	// Honor flint's [files] scoping so a diff compares like with like.
	var scope *Scope
	if !*noScope {
		scopeRoot := *repo
		if scopeRoot == "" && len(files) > 0 {
			scopeRoot = files[0]
		}
		if scopeRoot != "" {
			if scope, err = LoadScope(scopeRoot); err != nil {
				fmt.Fprintf(os.Stderr, "gitops-oracle: reading .fleetlint.toml: %v\n", err)
				os.Exit(2)
			}
		}
	}
	var inScope []string
	skipped := 0
	for _, f := range files {
		if scope.OutOfScope(f) {
			skipped++
			continue
		}
		inScope = append(inScope, f)
	}
	files = inScope
	if len(files) == 0 {
		fmt.Fprintln(os.Stderr, "gitops-oracle: no input files; pass FILE... or -repo PATH")
		os.Exit(2)
	}

	report := Report{
		FleetModule:       fleetModuleVersion(),
		Premium:           *premium,
		SkippedOutOfScope: skipped,
		Files:             make([]FileResult, 0, len(files)),
	}
	if scope != nil {
		report.ScopeConfig = scope.ConfigPath
		report.ScopeInclude = scope.Include
		report.ScopeExclude = scope.Exclude
	}

	appConfig := buildAppConfig(*premium)

	for _, f := range files {
		report.Files = append(report.Files, inspect(f, *base, appConfig))
	}

	for _, fr := range report.Files {
		report.Summary.Files++
		for _, fd := range fr.Findings {
			if fd.Severity == "error" {
				report.Summary.Errors++
			} else {
				report.Summary.Warnings++
			}
		}
	}

	enc := json.NewEncoder(os.Stdout)
	if *pretty {
		enc.SetIndent("", "  ")
	}
	if err := enc.Encode(report); err != nil {
		fmt.Fprintf(os.Stderr, "gitops-oracle: encoding report: %v\n", err)
		os.Exit(2)
	}

	// Exit 0 regardless of findings: this is an oracle, not a gate. The
	// caller decides what to do with disagreements.
}

// inspect runs Fleet's parser over one file and collects everything it says.
func inspect(path, baseOverride string, appConfig *fleet.EnrichedAppConfig) FileResult {
	res := FileResult{Path: path, Findings: []Finding{}}

	baseDir := filepath.Dir(path)
	if baseOverride != "" {
		baseDir = baseOverride
	}

	// Decide up front whether Fleet's parser is even the right judge for this
	// file. Feeding it a fragment produces an unmarshal error that looks like
	// a finding but only means "wrong entry point" — and fragments are the
	// majority of files in a real repo (213 of 214 in the reference repo are
	// profiles, standalone policy lists, software specs...).
	if kind := classify(path); kind != entryFile {
		res.NotEntryFile = true
		return res
	}

	// Fleet reports non-fatal observations (deprecated keys, globs that match
	// nothing, skipped files) through this log callback rather than as errors.
	// They are exactly the class flint should report as warnings, so capture
	// them instead of discarding.
	logFn := func(format string, a ...any) {
		msg := strings.TrimSpace(fmt.Sprintf(format, a...))
		// Fleet prefixes advisories with "[!] ".
		msg = strings.TrimPrefix(msg, "[!] ")
		if msg == "" {
			return
		}
		res.Findings = append(res.Findings, Finding{
			Severity: "warning",
			Message:  msg,
			Source:   "GitOpsFromFile",
		})
	}

	config, err := spec.GitOpsFromFile(path, baseDir, appConfig, logFn)
	if err != nil {
		msgs := flattenErr(err)
		if len(msgs) == 1 && isNotEntryFile(msgs[0]) {
			// Not a disagreement — this file is only meaningful via its parent.
			res.NotEntryFile = true
			res.Findings = []Finding{}
			return res
		}
		for _, msg := range msgs {
			res.Findings = append(res.Findings, Finding{
				Severity: "error",
				Message:  msg,
				Source:   "GitOpsFromFile",
			})
		}
		return res
	}

	res.Parsed = true
	res.TeamName = config.TeamName

	// Policy label scoping lives in the command layer, not in GitOpsFromFile
	// (cmd/fleetctl/fleetctl/gitops.go:1106). Run it here so the oracle covers
	// the same ground `fleetctl gitops` does.
	res.Findings = append(res.Findings, verifyPolicyLabelScopes(config)...)

	return res
}

// verifyPolicyLabelScopes mirrors getLabelUsage's per-policy check.
func verifyPolicyLabelScopes(config *spec.GitOps) []Finding {
	var out []Finding
	for _, policy := range config.Policies {
		if policy == nil {
			continue
		}
		if err := policy.VerifyLabelScopes(); err != nil {
			out = append(out, Finding{
				Severity: "error",
				Message:  fmt.Sprintf("Policy '%s': %v", policy.Name, err),
				Source:   "VerifyLabelScopes",
			})
		}
	}
	return out
}

// fileKind distinguishes the files Fleet's parser can judge alone from the
// ones it only ever sees through a parent's `path:`/`paths:`.
type fileKind int

const (
	// entryFile is a fleet file (`name:`) or the global config
	// (`org_settings:`) — what `fleetctl gitops -f` accepts.
	entryFile fileKind = iota
	// fragmentFile is anything else: a top-level sequence (standalone policy,
	// query, or label lists), a profile spec, a software spec. Fleet validates
	// these in the context of the file that references them.
	fragmentFile
	// unreadableFile could not be opened or parsed as YAML at all.
	unreadableFile
)

// classify peeks at a file's top-level YAML shape. This mirrors how
// GitOpsFromFile decides (parseName requires `name` or `org_settings`), but
// without invoking it, so a fragment never produces a phantom finding.
func classify(path string) fileKind {
	b, err := os.ReadFile(path)
	if err != nil {
		return unreadableFile
	}
	var top any
	if err := yaml.Unmarshal(b, &top); err != nil {
		return unreadableFile
	}
	m, ok := top.(map[string]any)
	if !ok {
		// Top-level sequence (or scalar/empty) — never an entry file.
		return fragmentFile
	}
	if _, hasName := m["name"]; hasName {
		return entryFile
	}
	if _, hasOrg := m["org_settings"]; hasOrg {
		return entryFile
	}
	return fragmentFile
}

// isNotEntryFile recognizes Fleet's "this is not a top-level gitops file"
// rejection. Matched on the stable, distinctive part of the message
// (pkg/spec/gitops.go parseName) rather than the whole string, which embeds
// the file path.
func isNotEntryFile(msg string) bool {
	return strings.Contains(msg, "No `name` was provided") &&
		strings.Contains(msg, "org_settings")
}

// flattenErr unwraps multierror so each Fleet complaint is its own finding —
// otherwise a file with six problems reads as one giant string and cannot be
// matched against flint's per-finding output.
func flattenErr(err error) []string {
	var me *multierror.Error
	if errors.As(err, &me) {
		out := make([]string, 0, len(me.Errors))
		for _, e := range me.Errors {
			out = append(out, strings.TrimSpace(e.Error()))
		}
		return out
	}
	return []string{strings.TrimSpace(err.Error())}
}

// buildAppConfig synthesizes the server-provided config GitOpsFromFile needs.
//
// EnrichedAppConfig embeds an unexported struct, so it cannot be built with a
// struct literal from outside package fleet — but its fields carry JSON tags,
// so unmarshaling reaches them.
func buildAppConfig(premium bool) *fleet.EnrichedAppConfig {
	tier := fleet.TierFree
	if premium {
		tier = fleet.TierPremium
	}
	var cfg fleet.EnrichedAppConfig
	raw := fmt.Sprintf(`{"license":{"tier":%q,"device_count":0}}`, tier)
	if err := json.Unmarshal([]byte(raw), &cfg); err != nil {
		panic(fmt.Sprintf("building app config: %v", err))
	}
	if cfg.License == nil {
		panic("license did not survive unmarshal — EnrichedAppConfig shape changed upstream")
	}
	return &cfg
}

// collectFiles resolves the input set, sorted for deterministic output.
func collectFiles(repo string, args []string) ([]string, error) {
	if repo == "" {
		return args, nil
	}
	var out []string
	err := filepath.WalkDir(repo, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			// Skip VCS and dependency dirs; also skip dot-dirs generally.
			name := d.Name()
			if name != "." && strings.HasPrefix(name, ".") {
				return filepath.SkipDir
			}
			if name == "node_modules" {
				return filepath.SkipDir
			}
			return nil
		}
		switch strings.ToLower(filepath.Ext(path)) {
		case ".yml", ".yaml":
			out = append(out, path)
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Strings(out)
	return out, nil
}

// fleetModuleVersion reports which Fleet the binary links, so a report can be
// attributed to a specific upstream revision.
func fleetModuleVersion() string {
	if v := readBuildModule("github.com/fleetdm/fleet/v4"); v != "" {
		return v
	}
	return "unknown (replace directive — see tools/gitops-oracle/README.md)"
}
