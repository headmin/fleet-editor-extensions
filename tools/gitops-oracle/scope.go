package main

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
	"github.com/bmatcuk/doublestar/v4"
)

// Scope mirrors flint's `[files]` include/exclude contract.
//
// A differential audit is only meaningful when both tools are asked about the
// same files. Without this, every scoping difference surfaces as a false gap:
// the first oracle run reported 10 "missing flint rules" that were entirely
// `tools-scripts/fleet-templates`, a directory flint was configured to ignore.
//
// Semantics copied from FleetLintConfig::is_out_of_scope_file
// (crates/flint-lint/src/config.rs):
//
//  1. `exclude` wins outright.
//  2. A non-empty `include` is AUTHORITATIVE — a path matching none of its
//     globs is out of scope, not merely un-narrowed.
//  3. No `include` configured means everything not excluded is in scope.
//
// flint's third rule (default to YAML-only) is deliberately NOT copied: the
// oracle already walks only *.yml/*.yaml, so applying it again would be a
// no-op that invites drift if flint's default ever changes.
type Scope struct {
	// Path of the .fleetlint.toml this came from; empty when none was found.
	ConfigPath string
	Root       string
	Include    []string
	Exclude    []string
}

// fleetLintConfig is the subset of .fleetlint.toml the oracle cares about.
// Unknown keys are ignored by design — this tool has no opinion on rules,
// thresholds or patterns, only on which files are in play.
type fleetLintConfig struct {
	Files struct {
		Include []string `toml:"include"`
		Exclude []string `toml:"exclude"`
	} `toml:"files"`
}

// LoadScope finds and reads the .fleetlint.toml governing `root`.
//
// Search walks upward from `root`, matching how flint discovers its config
// (FleetLintConfig::find_and_load), so pointing the oracle at a subdirectory
// picks up the repo-level scoping rather than silently linting everything.
func LoadScope(root string) (*Scope, error) {
	abs, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	dir := abs
	if info, err := os.Stat(abs); err == nil && !info.IsDir() {
		dir = filepath.Dir(abs)
	}

	for {
		candidate := filepath.Join(dir, ".fleetlint.toml")
		if _, err := os.Stat(candidate); err == nil {
			var cfg fleetLintConfig
			if _, err := toml.DecodeFile(candidate, &cfg); err != nil {
				return nil, err
			}
			return &Scope{
				ConfigPath: candidate,
				Root:       dir,
				Include:    cfg.Files.Include,
				Exclude:    cfg.Files.Exclude,
			}, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			// Reached the filesystem root without finding a config: nothing
			// is out of scope, which matches flint's no-config behavior.
			return &Scope{Root: abs}, nil
		}
		dir = parent
	}
}

// OutOfScope reports whether flint's `[files]` config excludes this path.
func (s *Scope) OutOfScope(path string) bool {
	if s == nil {
		return false
	}
	rel := s.relative(path)

	for _, pat := range s.Exclude {
		if matchGlob(pat, rel) {
			return true
		}
	}
	if len(s.Include) == 0 {
		return false
	}
	for _, pat := range s.Include {
		if matchGlob(pat, rel) {
			return false
		}
	}
	return true
}

// relative renders a path the way flint's globs are written: repo-relative,
// forward slashes, no leading "./".
func (s *Scope) relative(path string) string {
	p := path
	if abs, err := filepath.Abs(path); err == nil && s.Root != "" {
		if r, err := filepath.Rel(s.Root, abs); err == nil && !strings.HasPrefix(r, "..") {
			p = r
		}
	}
	p = filepath.ToSlash(p)
	return strings.TrimPrefix(p, "./")
}

// matchGlob applies one pattern. doublestar matches globset's
// literal_separator(true) behavior: `*` stops at `/`, `**` crosses it.
//
// A bare directory prefix is treated as covering everything beneath it, so
// `platforms/_retired` behaves like `platforms/_retired/**`. flint's globset
// requires the explicit `/**`; accepting both here keeps the oracle from
// reporting phantom gaps over a punctuation difference.
func matchGlob(pattern, path string) bool {
	pattern = strings.TrimPrefix(pattern, "./")
	if ok, err := doublestar.Match(pattern, path); err == nil && ok {
		return true
	}
	if !strings.ContainsAny(pattern, "*?[{") {
		prefix := strings.TrimSuffix(pattern, "/") + "/"
		return strings.HasPrefix(path, prefix)
	}
	return false
}
