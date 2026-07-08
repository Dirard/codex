package codex

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestInternalPackagesDoNotImportRootPackage(t *testing.T) {
	err := filepath.WalkDir("internal", func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() || !strings.HasSuffix(path, ".go") {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		if strings.Contains(string(data), `"github.com/openai/codex/sdk/go"`) {
			t.Fatalf("%s imports root SDK package", path)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}
