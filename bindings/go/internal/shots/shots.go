// Package shots says where tests save their screenshots, for a person to
// look at: target/go-shots in the repository.
package shots

import (
	"os"
	"path/filepath"
	"runtime"
)

// Path is where a test saves the screenshot called name.
func Path(name string) string {
	_, here, _, _ := runtime.Caller(0)
	dir := filepath.Join(filepath.Dir(here), "..", "..", "..", "..", "target", "go-shots")
	_ = os.MkdirAll(dir, 0o755)
	return filepath.Join(dir, name+".png")
}
