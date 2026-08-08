package main

import "runtime/debug"

// readBuildModule returns the version of a linked dependency, or "" when the
// build info is unavailable (e.g. `go run` in some modes) or the module was
// substituted by a `replace` directive pointing at a local checkout.
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
			// A local replace has no meaningful version; the caller falls
			// back to a message pointing at the README.
			return ""
		}
		return dep.Version
	}
	return ""
}
