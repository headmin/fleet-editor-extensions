// Command schema-extract enumerates every key Fleet's GitOps parser will
// accept, with the path it is valid at, by reflecting over Fleet's own Go
// types.
//
// # Why this exists
//
// flint's KEY_REGISTRY (crates/flint-lint/src/structure.rs) is a hand-written
// mirror of Fleet's schema, and every surface built on it — completions,
// hover, the "unknown key" and "belongs under X" diagnostics — is derived
// from the registry. That makes the registry's own staleness INVISIBLE to
// flint's internal guard tests: they check that the surfaces agree with each
// other, so a key missing from the registry is missing consistently and
// nothing fails. Only an external oracle catches it. That gap has bitten
// before: `labels_include_all` was unregistered for policies[], so the
// shipped binary emitted a BLOCKING "belongs under 'reports[]'" for valid
// YAML, and the org_logo_* keys were absent from every surface at once.
//
// The existing scripts/check-schema-coverage.sh greps `json:"..."` tags out
// of a fixed list of .go files. That is a flat key set with no paths, it
// cannot tell a GitOps key from an API response field, and it silently
// misses anything in a file nobody remembered to list. This walks the type
// graph from the actual GitOps entry points instead, so a key is reported if
// and only if Fleet's parser can reach it.
//
// # What "reachable" means
//
// gitops.go's parser validates each section against a specific Go type
// (validateYAMLKeys with reflect.TypeFor[GitOpsOrgSettings] and friends).
// The roots below are exactly those types. Everything reachable from them
// through exported fields is a legal key; anything else is not, no matter
// how many `json:` tags it carries elsewhere in the tree.
//
// # Usage
//
//	go run ./cmd/schema-extract > fleet-gitops-keys.json
//
// The Fleet version comes from this module's own go.mod pin (v4.89.2 — the
// version the target server runs), so the oracle, the Tier 2 harness and
// this extractor all describe one Fleet.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"reflect"
	"runtime/debug"
	"sort"
	"strings"

	"github.com/fleetdm/fleet/v4/pkg/spec"
	"github.com/fleetdm/fleet/v4/server/fleet"
)

// maxDepth bounds the walk. Fleet's config tree is shallow; anything deeper
// is a cycle we failed to detect, and silently truncating is better than
// hanging. Reported on stderr if it ever triggers.
const maxDepth = 12

// untagged collects exported fields with no json tag, reported on stderr.
// Deliberately package-level: it is diagnostic output, not part of the walk's
// contract.
var untagged = map[string]struct{}{}

// isJSONScalar reports whether t drives its own JSON decoding, in which case
// its Go fields are implementation detail rather than YAML keys.
func isJSONScalar(t reflect.Type) bool {
	u := reflect.TypeFor[json.Unmarshaler]()
	return t.Implements(u) || reflect.PointerTo(t).Implements(u)
}

// Key is one accepted key and where it is accepted.
type Key struct {
	// Key is the bare json name, e.g. "byod_team".
	Key string `json:"key"`
	// Path is the dot-separated parent path, "" for top level. Slice
	// elements are marked with "[]", matching KEY_REGISTRY's convention:
	// "org_settings.mdm.apple_business_manager[]".
	Path string `json:"path"`
	// Type is the Go type name, for triage when a key looks surprising.
	Type string `json:"type"`
}

// roots are Fleet's OWN key-validation sites, transcribed from the
// `validateYAMLKeys(..., reflect.TypeFor[T](), ..., []string{path})` calls in
// pkg/spec/gitops.go. Those calls are the definition of "this key is legal
// here" — they are literally what produces Fleet's "unknown top-level field"
// and unknown-key errors — so mirroring them is exact rather than inferred.
//
// Do not substitute types that merely look right. `policies:` validates
// against spec.Policy, NOT GitOpsPolicySpec; `reports:` against spec.Query,
// NOT fleet.QuerySpec. Those pairs differ, and guessing produced a key set
// that disagreed with Fleet in both directions.
//
// grep the pin to re-derive after a version bump:
//
//	grep -n 'validateYAMLKeys(' $(go env GOMODCACHE)/github.com/fleetdm/fleet/v4@VERSION/pkg/spec/gitops.go
var roots = []struct {
	path string
	typ  reflect.Type
}{
	// gitops.go:680 — embeds fleet.AppConfig, so the whole app config tree
	// hangs off here.
	{"org_settings", reflect.TypeFor[spec.GitOpsOrgSettings]()},
	// gitops.go:909. `team_settings:` is the deprecated spelling Fleet still
	// accepts, validated against the same type.
	{"settings", reflect.TypeFor[spec.GitOpsFleetSettings]()},
	{"team_settings", reflect.TypeFor[spec.GitOpsFleetSettings]()},
	// gitops.go:1413. NOTE: most GitOpsControls fields are typed `any`, so
	// reflection stops at the section's own keys — the nested shapes
	// (macos_updates, macos_setup, …) are validated elsewhere and are NOT
	// covered here. Treat controls sub-keys as unverified by this tool.
	{"controls", reflect.TypeFor[spec.GitOpsControls]()},
	{"labels[]", reflect.TypeFor[spec.Label]()},
	{"policies[]", reflect.TypeFor[spec.Policy]()},
	{"reports[]", reflect.TypeFor[spec.Query]()},
	{"queries[]", reflect.TypeFor[spec.Query]()},
	// gitops.go:1936/2232 validate software.packages against BOTH of these,
	// so a key accepted by either is accepted.
	{"software.packages[]", reflect.TypeFor[fleet.SoftwarePackageSpec]()},
	{"software.packages[]", reflect.TypeFor[spec.SoftwarePackage]()},
}

// softwareSections are the `software:` children. GitOpsSoftware — the struct
// the parse RESULT lands in — has untagged fields, but the YAML shape is
// tagged at gitops.go:326-328. Named explicitly so the walk covers them
// without inventing "Packages:" from a Go field name.
var softwareSections = []struct {
	path string
	typ  reflect.Type
}{
	{"software.app_store_apps[]", reflect.TypeFor[fleet.TeamSpecAppStoreApp]()},
	{"software.fleet_maintained_apps[]", reflect.TypeFor[fleet.MaintainedAppSpec]()},
}

func main() {
	out := map[string]Key{}
	truncated := 0

	for _, r := range roots {
		walk(r.typ, r.path, out, 0, &truncated, map[reflect.Type]bool{})
	}
	for _, r := range softwareSections {
		walk(r.typ, r.path, out, 0, &truncated, map[reflect.Type]bool{})
	}
	// `software:` children, whose names live in the tagged YAML struct rather
	// than in GitOpsSoftware.
	for _, k := range []string{"packages", "app_store_apps", "fleet_maintained_apps"} {
		out[k+"@software"] = Key{Key: k, Path: "software", Type: "GitOpsSoftware"}
	}

	// Top-level keys are not reachable by reflection — gitops.go lists them
	// as a literal string slice — so they are transcribed here from
	// `topKeys` in ValidateGitOps. Kept adjacent to the roots above so the
	// two cannot drift apart unnoticed.
	for _, k := range []string{
		"name", "settings", "org_settings", "agent_options", "controls",
		"policies", "reports", "queries", "software", "labels",
		"custom_host_vitals",
	} {
		out[k+"@"] = Key{Key: k, Path: "", Type: "topKeys"}
	}

	keys := make([]Key, 0, len(out))
	for _, v := range out {
		keys = append(keys, v)
	}
	sort.Slice(keys, func(i, j int) bool {
		if keys[i].Path != keys[j].Path {
			return keys[i].Path < keys[j].Path
		}
		return keys[i].Key < keys[j].Key
	})

	if len(untagged) > 0 {
		names := make([]string, 0, len(untagged))
		for n := range untagged {
			names = append(names, n)
		}
		sort.Strings(names)
		fmt.Fprintf(os.Stderr, "note: %d exported field(s) with no json tag, skipped: %s\n",
			len(names), strings.Join(names, ", "))
	}
	if truncated > 0 {
		fmt.Fprintf(os.Stderr, "warning: %d branch(es) hit maxDepth=%d\n", truncated, maxDepth)
	}

	doc := struct {
		FleetVersion string `json:"fleet_version"`
		Count        int    `json:"count"`
		Keys         []Key  `json:"keys"`
	}{
		FleetVersion: readBuildModule("github.com/fleetdm/fleet/v4"),
		Count:        len(keys),
		Keys:         keys,
	}

	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(doc); err != nil {
		fmt.Fprintln(os.Stderr, "encode:", err)
		os.Exit(1)
	}
}

// walk records every json key reachable from t at yamlPath.
//
// `seen` is per-branch, not global: the same type legitimately appears at
// several paths (WebhookSettings under both org_settings and team settings),
// and a global visited-set would record it only at whichever path the walk
// happened to reach first.
func walk(t reflect.Type, yamlPath string, out map[string]Key, depth int, truncated *int, seen map[reflect.Type]bool) {
	if depth > maxDepth {
		*truncated++
		return
	}
	t = deref(t)
	if t.Kind() != reflect.Struct {
		return
	}
	// A type with its own UnmarshalJSON is a SCALAR as far as YAML is
	// concerned, whatever its Go fields look like. Fleet uses optjson.Bool,
	// optjson.String and null.* extensively; each is a struct of
	// {Set, Valid, Value}. Walking into them invents `set:`, `valid:` and
	// `value:` as legal keys at dozens of paths — keys Fleet would reject.
	// Over-approximation here is not the safe direction: it would teach
	// flint to accept YAML the server refuses.
	if isJSONScalar(t) {
		return
	}
	if seen[t] {
		return // recursive type on this branch
	}
	seen[t] = true
	defer delete(seen, t)

	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		if f.PkgPath != "" {
			continue // unexported
		}

		name, opts := parseJSONTag(f.Tag.Get("json"))
		if name == "-" {
			continue
		}

		// An embedded struct with no json name is INLINED by encoding/json:
		// its fields belong to the parent, at the parent's path. This is how
		// GitOpsOrgSettings exposes all of fleet.AppConfig, so getting it
		// wrong would lose most of the schema.
		if f.Anonymous && name == "" {
			walk(f.Type, yamlPath, out, depth, truncated, seen)
			continue
		}
		if name == "" {
			// An exported field with no json tag. encoding/json would use the
			// Go field name verbatim ("Packages"), but GitOps YAML is
			// lower_snake throughout and these fields are populated by
			// gitops.go's own parsing rather than by struct unmarshalling.
			// Emitting the Go name would add keys like `Packages:` and
			// `AppStoreApps:` that Fleet does not accept. Count and report
			// instead of guessing — if one of these ever IS a real YAML key,
			// the stderr line is the prompt to handle it deliberately.
			untagged[typeName(t)+"."+f.Name] = struct{}{}
			continue
		}
		_ = opts

		out[name+"@"+yamlPath] = Key{
			Key:  name,
			Path: yamlPath,
			Type: typeName(f.Type),
		}

		// Recurse into the value. A slice/array of structs becomes "path[]",
		// matching how KEY_REGISTRY spells list items.
		child := deref(f.Type)
		childPath := join(yamlPath, name)
		switch child.Kind() {
		case reflect.Slice, reflect.Array:
			elem := deref(child.Elem())
			if elem.Kind() == reflect.Struct {
				walk(elem, childPath+"[]", out, depth+1, truncated, seen)
			}
		case reflect.Map:
			// A map means arbitrary user keys (labels, secrets); its VALUE
			// type can still carry a fixed shape.
			elem := deref(child.Elem())
			if elem.Kind() == reflect.Struct {
				walk(elem, childPath+".*", out, depth+1, truncated, seen)
			}
		case reflect.Struct:
			walk(child, childPath, out, depth+1, truncated, seen)
		}
	}
}

func deref(t reflect.Type) reflect.Type {
	for t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	return t
}

func join(path, name string) string {
	if path == "" {
		return name
	}
	return path + "." + name
}

func parseJSONTag(tag string) (string, string) {
	if tag == "" {
		return "", ""
	}
	parts := strings.SplitN(tag, ",", 2)
	if len(parts) == 1 {
		return parts[0], ""
	}
	return parts[0], parts[1]
}

// readBuildModule returns the version of a linked dependency. Duplicated from
// the oracle's buildinfo.go rather than shared: this is a separate `main`
// package, and stamping the extracted schema with the Fleet version it came
// from matters more than avoiding twelve lines. An empty result means the
// module was replaced by a local checkout, in which case the version is not
// meaningful.
func readBuildModule(path string) string {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return ""
	}
	for _, dep := range info.Deps {
		if dep.Path != path {
			continue
		}
		if dep.Replace != nil {
			return ""
		}
		return dep.Version
	}
	return ""
}

func typeName(t reflect.Type) string {
	if t.Kind() == reflect.Ptr {
		return "*" + typeName(t.Elem())
	}
	if t.Name() != "" {
		return t.Name()
	}
	return t.String()
}
