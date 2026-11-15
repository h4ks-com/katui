package katui

// Rust is the source of truth for the API. Every public widget, game item,
// type method, theme field, trait hook and Control and Edit variant in the
// Rust crates has a Go counterpart, or an entry in omitted saying why Go
// does without it. Adding something public in Rust fails this test until
// the binding or the reason is written.

import (
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

var renamed = map[string]string{
	"text_field":    "TextFieldView",
	"text_area":     "TextAreaView",
	"toasts":        "ToastsView",
	"markdown_view": "Markdown",
	"image":         "ImageView",
	"Column":        "TableColumn",
	"HeldKeys":      "Keys",
	"P2":            "Point",
	"Rgb":           "RGB",
	"HtmlDefaults":  "Document",
	"HtmlHost":      "Document",
	"with_text":     "New",
	"to_surface":    "ToImage",
}

var omitted = map[string]string{
	"Key":              "keys are named by strings like \"a\", \"enter\" and \"up\"",
	"Theme.scope":      "the app runner and Screen set the theme around every view",
	"HeldKeys.tick":    "the game loop feeds the held keys",
	"HeldKeys.press":   "the game loop feeds the held keys",
	"HeldKeys.release": "the game loop feeds the held keys",
	"HeldKeys.clear":   "the game loop feeds the held keys",
}

var initialisms = map[string]string{"id": "ID", "xy": "XY", "html": "HTML", "srgb": "SRGB", "rgb": "RGB"}

// camel is a Rust snake_case or SCREAMING_CASE name in Go's MixedCaps.
func camel(name string) string {
	var parts []string
	for _, part := range strings.Split(strings.ToLower(name), "_") {
		if initialism, ok := initialisms[part]; ok {
			parts = append(parts, initialism)
		} else if part != "" {
			parts = append(parts, strings.ToUpper(part[:1])+part[1:])
		}
	}
	return strings.Join(parts, "")
}

func goName(name string) string {
	if to, ok := renamed[name]; ok {
		return to
	}
	if name[0] >= 'A' && name[0] <= 'Z' && strings.ToUpper(name) != name {
		return name
	}
	return camel(name)
}

// api is every exported name of the package.
type api struct {
	names      map[string]bool
	methods    map[string]map[string]bool
	interfaces map[string]bool
}

func parseAPI(t *testing.T) api {
	t.Helper()
	found := api{names: map[string]bool{}, methods: map[string]map[string]bool{}, interfaces: map[string]bool{}}
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	fset := token.NewFileSet()
	for _, path := range files {
		if strings.HasSuffix(path, "_test.go") {
			continue
		}
		file, err := parser.ParseFile(fset, path, nil, 0)
		if err != nil {
			t.Fatal(err)
		}
		found.add(file)
	}
	return found
}

func (a api) add(file *ast.File) {
	for _, decl := range file.Decls {
		switch decl := decl.(type) {
		case *ast.FuncDecl:
			if decl.Recv == nil {
				a.names[decl.Name.Name] = true
				continue
			}
			receiver := strings.TrimPrefix(types(decl.Recv.List[0].Type), "*")
			if a.methods[receiver] == nil {
				a.methods[receiver] = map[string]bool{}
			}
			a.methods[receiver][decl.Name.Name] = true
		case *ast.GenDecl:
			a.addSpecs(decl.Specs)
		}
	}
}

func (a api) addSpecs(specs []ast.Spec) {
	for _, spec := range specs {
		switch spec := spec.(type) {
		case *ast.TypeSpec:
			a.names[spec.Name.Name] = true
			if shape, ok := spec.Type.(*ast.InterfaceType); ok {
				for _, method := range shape.Methods.List {
					for _, name := range method.Names {
						a.interfaces[name.Name] = true
					}
				}
			}
		case *ast.ValueSpec:
			for _, name := range spec.Names {
				a.names[name.Name] = true
			}
		}
	}
}

func types(expr ast.Expr) string {
	if star, ok := expr.(*ast.StarExpr); ok {
		return "*" + types(star.X)
	}
	if ident, ok := expr.(*ast.Ident); ok {
		return ident.Name
	}
	return ""
}

func read(t *testing.T, path string) string {
	t.Helper()
	text, err := os.ReadFile(filepath.Join("..", "..", "crates", path))
	if err != nil {
		t.Fatal(err)
	}
	return string(text)
}

// exports is the names a crate's lib.rs re-exports with `pub use`.
func exports(source string) []string {
	var names []string
	for _, block := range regexp.MustCompile(`(?m)^pub use [\w:]+::(\{[^}]*\}|\w+);`).FindAllStringSubmatch(source, -1) {
		names = append(names, regexp.MustCompile(`\w+`).FindAllString(block[1], -1)...)
	}
	return names
}

// publicFns is the `pub fn` names in the inherent impl blocks of typeName.
func publicFns(source, typeName string) []string {
	var names []string
	impl := regexp.MustCompile(`(?m)^impl(?:<[^{]*?>)?\s+` + typeName + `(?:<[^>]*>)?\s*\{`)
	for _, start := range impl.FindAllStringIndex(source, -1) {
		depth, end := 0, start[1]-1
		for end = start[1] - 1; end < len(source); end++ {
			depth += map[byte]int{'{': 1, '}': -1}[source[end]]
			if depth == 0 {
				break
			}
		}
		for _, match := range regexp.MustCompile(`(?m)^    pub (?:const )?fn (\w+)`).FindAllStringSubmatch(source[start[1]:end], -1) {
			names = append(names, match[1])
		}
	}
	return names
}

func TestEveryRustExportHasAGoName(t *testing.T) {
	found := parseAPI(t)
	for _, lib := range []string{"katui-widgets", "katui-game", "katui-content"} {
		for _, name := range exports(read(t, lib+"/src/lib.rs")) {
			if _, skip := omitted[name]; !skip && !found.names[goName(name)] {
				t.Errorf("%s exports %s, and Go has no %s", lib, name, goName(name))
			}
		}
	}
}

var methodSources = [][2]string{
	{"katui-ui/src/element.rs", "Element"},
	{"katui-widgets/src/theme.rs", "Theme"},
	{"katui-widgets/src/field.rs", "TextField"},
	{"katui-widgets/src/area.rs", "TextArea"},
	{"katui-widgets/src/list.rs", "ListState"},
	{"katui-widgets/src/scroll.rs", "ScrollState"},
	{"katui-widgets/src/tree.rs", "TreeNode"},
	{"katui-widgets/src/tree.rs", "TreeState"},
	{"katui-widgets/src/focus.rs", "Focus"},
	{"katui-widgets/src/feedback.rs", "Toasts"},
	{"katui-game/src/light.rs", "Light"},
	{"katui-game/src/keys.rs", "HeldKeys"},
	{"katui-game/src/space.rs", "V3"},
	{"katui-game/src/space.rs", "Camera"},
	{"katui-game/src/space.rs", "Rng"},
	{"katui-game/src/space.rs", "Sparks"},
}

// hasMethod reports whether Go has rust's method as a method of its type,
// or as a function named after the method, alone or with the type.
func (a api) hasMethod(rust, method string) bool {
	typeName, name := goName(rust), goName(method)
	if method == "new" || method == "with_text" {
		return a.names["New"+typeName]
	}
	return a.methods[typeName][name] || a.names[name] || a.names[name+typeName] || a.names[typeName+name]
}

func TestEveryRustMethodHasAGoOne(t *testing.T) {
	found := parseAPI(t)
	for _, pair := range methodSources {
		source, rust := pair[0], pair[1]
		names := publicFns(read(t, source), rust)
		if len(names) == 0 {
			t.Fatalf("no public methods of %s found; fix the parser or the path", rust)
		}
		for _, method := range names {
			if _, skip := omitted[rust+"."+method]; !skip && !found.hasMethod(rust, method) {
				t.Errorf("Go has no counterpart of %s::%s", rust, method)
			}
		}
	}
}

func TestEveryThemeFieldIsAGoMethod(t *testing.T) {
	found := parseAPI(t)
	theme := regexp.MustCompile(`(?s)pub struct Theme \{(.*?)\n\}`).FindStringSubmatch(read(t, "katui-widgets/src/theme.rs"))
	if theme == nil {
		t.Fatal("no Theme struct found")
	}
	for _, field := range regexp.MustCompile(`(?m)^    pub (\w+):`).FindAllStringSubmatch(theme[1], -1) {
		wanted := []string{camel(field[1])}
		if field[1] != "cell" {
			wanted = append(wanted, "Set"+camel(field[1]))
		}
		for _, name := range wanted {
			if !found.methods["Theme"][name] {
				t.Errorf("Theme has no %s for the field %s", name, field[1])
			}
		}
	}
}

func TestEveryHookOfARustTraitIsAGoInterfaceMethod(t *testing.T) {
	found := parseAPI(t)
	for _, pair := range [][2]string{{"katui-app/src/lib.rs", "UiApp"}, {"katui-game/src/arcade.rs", "Game"}} {
		block := regexp.MustCompile(`(?ms)^pub trait ` + pair[1] + `\b.*?^\}`).FindString(read(t, pair[0]))
		if block == "" {
			t.Fatalf("trait %s not found", pair[1])
		}
		for _, hook := range regexp.MustCompile(`(?m)^    fn (\w+)`).FindAllStringSubmatch(block, -1) {
			if !found.interfaces[camel(hook[1])] {
				t.Errorf("no Go interface has %s for %s::%s", camel(hook[1]), pair[1], hook[1])
			}
		}
	}
}

func TestEveryRustVariantHasAGoConstant(t *testing.T) {
	found := parseAPI(t)
	for _, pair := range [][2]string{{"katui-rt/src/app.rs", "Control"}, {"katui-widgets/src/field.rs", "Edit"}} {
		block := regexp.MustCompile(`(?s)pub enum ` + pair[1] + ` \{(.*?)\n\}`).FindStringSubmatch(read(t, pair[0]))
		if block == nil {
			t.Fatalf("enum %s not found", pair[1])
		}
		for _, variant := range regexp.MustCompile(`(?m)^    (\w+),`).FindAllStringSubmatch(block[1], -1) {
			if !found.names[variant[1]] {
				t.Errorf("Go has no constant for %s::%s", pair[1], variant[1])
			}
		}
	}
}

func TestTheVersionIsTheWorkspaceVersion(t *testing.T) {
	workspace, err := os.ReadFile(filepath.Join("..", "..", "Cargo.toml"))
	if err != nil {
		t.Fatal(err)
	}
	version := regexp.MustCompile(`(?m)^version = "([^"]+)"`).FindStringSubmatch(string(workspace))
	if version == nil || version[1] != Version() {
		t.Fatalf("the engine is %s, the workspace %v", Version(), version)
	}
}
