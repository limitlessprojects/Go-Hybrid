// Command nativehook is an opt-in native Go scalar/channel safety experiment.
package main

import (
	"bytes"
	"crypto/sha256"
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: nativehook TOOL ARGS...")
		os.Exit(2)
	}
	os.Exit(run(os.Args[1], os.Args[2:]))
}
func run(tool string, args []string) int {
	name := strings.TrimSuffix(filepath.Base(tool), ".exe")
	if name == "compile" && !contains(args, "-V=full") {
		if err := check(args); err != nil {
			fmt.Fprintln(os.Stderr, "HYB-NATIVE-E001:", err)
			return 1
		}
	}
	cmd := exec.Command(tool, args...)
	cmd.Stdin = os.Stdin
	cmd.Stderr = os.Stderr
	if contains(args, "-V=full") {
		output, err := cmd.Output()
		if err != nil {
			return exitStatus(err)
		}
		self, err := os.Executable()
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		data, err := os.ReadFile(self)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		hash := sha256.Sum256(data)
		fmt.Printf("%s native-scalar-v1-%x\n", strings.TrimSpace(string(output)), hash[:12])
		return 0
	}
	cmd.Stdout = os.Stdout
	return exitStatus(cmd.Run())
}
func exitStatus(err error) int {
	if err == nil {
		return 0
	}
	if e, ok := err.(*exec.ExitError); ok {
		return e.ExitCode()
	}
	fmt.Fprintln(os.Stderr, err)
	return 1
}
func contains(args []string, value string) bool {
	for _, arg := range args {
		if arg == value {
			return true
		}
	}
	return false
}
func flag(args []string, name string) string {
	for i, arg := range args {
		if arg == name && i+1 < len(args) {
			return args[i+1]
		}
		if strings.HasPrefix(arg, name+"=") {
			return strings.TrimPrefix(arg, name+"=")
		}
	}
	return ""
}
func safeType(t types.Type) bool {
	switch t := t.Underlying().(type) {
	case *types.Basic:
		return t.Kind() != types.UnsafePointer
	case *types.Chan:
		b, ok := t.Elem().Underlying().(*types.Basic)
		return ok && b.Kind() != types.UnsafePointer
	}
	return false
}

func check(args []string) error {
	var paths []string
	enabled := false
	for _, arg := range args {
		if strings.HasSuffix(arg, ".go") {
			data, err := os.ReadFile(arg)
			if err != nil {
				return err
			}
			paths = append(paths, arg)
			if bytes.Contains(data, []byte("//hybrid:checked")) {
				enabled = true
			}
		}
	}
	if !enabled {
		return nil
	}
	fset := token.NewFileSet()
	files := []*ast.File{}
	hasMarker := false
	for _, path := range paths {
		file, err := parser.ParseFile(fset, path, nil, parser.ParseComments)
		if err != nil {
			return err
		}
		files = append(files, file)
		for _, group := range file.Comments {
			for _, comment := range group.List {
				if strings.TrimSpace(comment.Text) == "//hybrid:checked" {
					hasMarker = true
				}
			}
		}
	}
	if !hasMarker {
		return nil
	}
	exports := map[string]string{}
	aliases := map[string]string{}
	if cfg := flag(args, "-importcfg"); cfg != "" {
		data, err := os.ReadFile(cfg)
		if err != nil {
			return err
		}
		for _, line := range strings.Split(string(data), "\n") {
			parts := strings.SplitN(strings.TrimSpace(line), " ", 2)
			if len(parts) != 2 {
				continue
			}
			pair := strings.SplitN(parts[1], "=", 2)
			if len(pair) != 2 {
				continue
			}
			if parts[0] == "packagefile" {
				exports[pair[0]] = pair[1]
			}
			if parts[0] == "importmap" {
				aliases[pair[0]] = pair[1]
			}
		}
	}
	lookup := func(path string) (io.ReadCloser, error) {
		if alias := aliases[path]; alias != "" {
			path = alias
		}
		file, ok := exports[path]
		if !ok {
			return nil, fmt.Errorf("missing export data for %s", path)
		}
		return os.Open(file)
	}
	info := &types.Info{Types: map[ast.Expr]types.TypeAndValue{}, Defs: map[*ast.Ident]types.Object{}, Uses: map[*ast.Ident]types.Object{}}
	config := types.Config{Importer: importer.ForCompiler(fset, "gc", lookup)}
	pkg, err := config.Check(flag(args, "-p"), fset, files, info)
	if err != nil {
		return fmt.Errorf("native type analysis: %w", err)
	}
	checked := map[*types.Func]*ast.FuncDecl{}
	for _, file := range files {
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Doc == nil {
				continue
			}
			for _, comment := range fn.Doc.List {
				if strings.TrimSpace(comment.Text) == "//hybrid:checked" {
					obj, _ := info.Defs[fn.Name].(*types.Func)
					if obj == nil {
						return fmt.Errorf("unresolved checked function")
					}
					checked[obj] = fn
				}
			}
		}
	}
	if len(checked) == 0 {
		return fmt.Errorf("checked annotation must attach to a function declaration")
	}
	for obj, fn := range checked {
		reject := func(message string) error {
			return fmt.Errorf("%s: %s: %s", fset.Position(fn.Pos()), obj.Name(), message)
		}
		if fn.Body == nil || fn.Recv != nil || fn.Type.TypeParams != nil {
			return reject("only concrete functions with bodies are admitted")
		}
		signature := obj.Type().(*types.Signature)
		for _, tuple := range []*types.Tuple{signature.Params(), signature.Results()} {
			for i := 0; i < tuple.Len(); i++ {
				if !safeType(tuple.At(i).Type()) {
					return reject("only scalar values and scalar channels are admitted")
				}
			}
		}
		var failure error
		ast.Inspect(fn.Body, func(node ast.Node) bool {
			if failure != nil {
				return false
			}
			fail := func(why string) { failure = reject(why) }
			switch n := node.(type) {
			case *ast.FuncLit:
				fail("closures are outside the admitted subset")
			case *ast.SelectorExpr:
				fail("selectors and foreign effects are outside the admitted subset")
			case *ast.StarExpr:
				fail("pointer dereference is not admitted")
			case *ast.UnaryExpr:
				if n.Op == token.AND {
					fail("address escape is not admitted")
				}
			case *ast.Ident:
				if variable, ok := info.Uses[n].(*types.Var); ok && variable.Parent() == pkg.Scope() {
					fail("mutable global access is not admitted")
				}
				if variable, ok := info.Defs[n].(*types.Var); ok && !safeType(variable.Type()) {
					fail("reference-bearing local is not admitted")
				}
			case *ast.CallExpr:
				id, ok := n.Fun.(*ast.Ident)
				if !ok {
					fail("indirect calls are not admitted")
					break
				}
				switch target := info.Uses[id].(type) {
				case *types.Func:
					if checked[target] == nil {
						fail("call to an unchecked function")
					}
				case *types.Builtin:
					switch target.Name() {
					case "len", "cap", "close", "make", "min", "max":
					default:
						fail("builtin is not admitted")
					}
					if target.Name() == "make" && !safeType(info.TypeOf(n)) {
						fail("only scalar channel allocation is admitted")
					}
				case *types.TypeName:
					if !safeType(info.TypeOf(n)) {
						fail("non-scalar conversion is not admitted")
					}
				default:
					fail("unresolved or indirect call")
				}
			}
			return failure == nil
		})
		if failure != nil {
			return failure
		}
	}
	return nil
}
