package main

import (
	"os"
	"path/filepath"
	"testing"
)

// A dot-prefixed ROOT must be walked. Only dot-directories *inside* the tree
// are tooling to skip; the root is whatever the caller pointed at — and
// tempfile-style scratch dirs are dot-prefixed by default.
func TestCollectFilesWalksADotPrefixedRoot(t *testing.T) {
	root := filepath.Join(t.TempDir(), ".scratch")
	if err := os.MkdirAll(filepath.Join(root, "fleets"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "fleets", "a.yml"), []byte("name: A\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	// A dot-directory INSIDE the root is still skipped.
	if err := os.MkdirAll(filepath.Join(root, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, ".git", "x.yml"), []byte("nope: 1\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	files, err := collectFiles(root, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 1 || filepath.Base(files[0]) != "a.yml" {
		t.Fatalf("expected exactly fleets/a.yml, got %v", files)
	}
}
