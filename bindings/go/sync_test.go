package katui

// Rust is the source of truth for the API. Every public widget, game item,
// type method, theme field, trait hook and Control and Edit variant in the
// Rust crates has a Go counterpart, or an entry in omitted saying why Go
// does without it. Adding something public in Rust fails this test until
// the binding or the reason is written.

import (
	"encoding/json"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
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

var initialisms = map[string]string{"id": "ID", "xy": "XY", "html": "HTML", "srgb": "SRGB", "rgb": "RGB", "ui": "UI"}

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

// rustAPI is bindings/rust-api.json: the public Rust API every binding covers.
type rustAPI struct {
	Version  string              `json:"version"`
	Exports  map[string][]string `json:"exports"`
	Methods  map[string][]string `json:"methods"`
	Fields   map[string][]string `json:"fields"`
	Hooks    map[string][]string `json:"hooks"`
	Variants map[string][]string `json:"variants"`
}

func readAPI(t *testing.T) rustAPI {
	t.Helper()
	text, err := os.ReadFile(filepath.Join("..", "rust-api.json"))
	if err != nil {
		t.Fatal(err)
	}
	var found rustAPI
	if err := json.Unmarshal(text, &found); err != nil {
		t.Fatal(err)
	}
	return found
}

func TestEveryRustExportHasAGoName(t *testing.T) {
	found := parseAPI(t)
	for crate, names := range readAPI(t).Exports {
		for _, name := range names {
			if _, skip := omitted[name]; !skip && !found.names[goName(name)] {
				t.Errorf("%s exports %s, and Go has no %s", crate, name, goName(name))
			}
		}
	}
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
	for rust, names := range readAPI(t).Methods {
		for _, method := range names {
			if _, skip := omitted[rust+"."+method]; !skip && !found.hasMethod(rust, method) {
				t.Errorf("Go has no counterpart of %s::%s", rust, method)
			}
		}
	}
}

func TestEveryThemeFieldIsAGoMethod(t *testing.T) {
	found := parseAPI(t)
	for _, field := range readAPI(t).Fields["Theme"] {
		wanted := []string{camel(field)}
		if field != "cell" {
			wanted = append(wanted, "Set"+camel(field))
		}
		for _, name := range wanted {
			if !found.methods["Theme"][name] {
				t.Errorf("Theme has no %s for the field %s", name, field)
			}
		}
	}
}

func TestEveryHookOfARustTraitIsAGoInterfaceMethod(t *testing.T) {
	found := parseAPI(t)
	for trait, hooks := range readAPI(t).Hooks {
		for _, hook := range hooks {
			if !found.interfaces[camel(hook)] {
				t.Errorf("no Go interface has %s for %s::%s", camel(hook), trait, hook)
			}
		}
	}
}

func TestEveryRustVariantHasAGoConstant(t *testing.T) {
	found := parseAPI(t)
	for enum, variants := range readAPI(t).Variants {
		for _, variant := range variants {
			if !found.names[variant] {
				t.Errorf("Go has no constant for %s::%s", enum, variant)
			}
		}
	}
}

func TestTheVersionIsTheWorkspaceVersion(t *testing.T) {
	if want := readAPI(t).Version; want != Version() {
		t.Fatalf("the engine is %s, the workspace %s", Version(), want)
	}
}
