package codex

import "testing"

func isolateTestCodexHome(t *testing.T) string {
	t.Helper()
	codexHome := t.TempDir()
	t.Setenv("CODEX_HOME", codexHome)
	return codexHome
}
