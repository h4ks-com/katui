package katui

// The Go code in the docs keeps working: every block is a program that
// builds, and a block that runs an app or a game runs, with Run and Play
// drawing one frame off-terminal.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

const headless = `
func headlessRun(app katui.App) error {
	screen, err := katui.NewScreen(app, 600, 300)
	if err != nil {
		return err
	}
	defer screen.Close()
	_, err = screen.Render()
	return err
}

func headlessPlay(game katui.Game) error {
	_, err := katui.Frame(game, katui.FrameOptions{Steps: 3, Held: []string{"right"}})
	return err
}
`

func goBlocks(t *testing.T, path string) []string {
	t.Helper()
	text, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var blocks []string
	for _, match := range regexp.MustCompile("(?ms)^```go\n(.*?)^```").FindAllStringSubmatch(string(text), -1) {
		blocks = append(blocks, match[1])
	}
	if len(blocks) == 0 {
		t.Fatalf("%s has no Go blocks", path)
	}
	return blocks
}

func TestTheGoBlocksInTheDocsBuildAndRun(t *testing.T) {
	module, err := filepath.Abs(".")
	if err != nil {
		t.Fatal(err)
	}
	for _, path := range []string{filepath.Join("..", "..", "docs", "go.md"), "README.md"} {
		for index, code := range goBlocks(t, path) {
			t.Run(fmt.Sprintf("%s:%d", filepath.Base(path), index), func(t *testing.T) {
				runBlock(t, module, code)
			})
		}
	}
}

func runBlock(t *testing.T, module, code string) {
	dir := t.TempDir()
	runs := strings.Contains(code, "katui.Run(") || strings.Contains(code, "katui.Play(")
	if runs {
		code = strings.NewReplacer("katui.Run(", "headlessRun(", "katui.Play(", "headlessPlay(").Replace(code) + headless
	}
	goMod := fmt.Sprintf("module docs\n\ngo 1.24\n\nrequire github.com/h4ks-com/katui/bindings/go v0.0.0\n\n"+
		"replace github.com/h4ks-com/katui/bindings/go => %s\n", module)
	for name, text := range map[string]string{"go.mod": goMod, "main.go": code} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	command := []string{"vet", "."}
	if runs {
		command = []string{"run", "."}
	}
	cmd := exec.Command("go", command...)
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("go %s: %v\n%s\n%s", command[0], err, out, code)
	}
}
