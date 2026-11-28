// Package model extracts a bounded concurrency model from *emitted Go*.
//
// The input is the Go file that `hyb build --emit-go` produced, i.e. the
// exact bytes the Go toolchain compiles and runs. Nothing is read from the
// .hy source or from its EXPECTED-VERDICT comment header.
package model

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"strconv"
	"strings"
)

// Kind classifies how a value behaves across a task boundary.
type Kind int

const (
	KindCopy Kind = iota // value copy: no alias
	KindShared
	KindChan
	KindMutex
	KindAtomic
	KindUnknown
)

// Desc describes one bound argument or variable.
type Desc struct {
	Kind   Kind
	Name   string   // syntactic root, used as the location identity
	Cap    int      // channel capacity
	Fields []string // reference-typed field names of a struct value
}

// StepKind is one transition of a task.
type StepKind int

const (
	Nop StepKind = iota
	Spawn
	Send
	Recv
	Close
	Lock
	Unlock
	Write
	Read
)

// Step is one transition in a task's straight-line program.
type Step struct {
	Kind  StepKind
	Task  int    // spawn target task id
	Chan  string // channel name
	Mutex string // mutex name
	Loc   string // shared location name
	Label string
}

// Task is one goroutine: main plus each spawned callee.
type Task struct {
	ID    int
	Name  string
	Steps []Step
}

// Program is the extracted bounded model.
type Program struct {
	Name       string
	GoSHA      string
	GoSource   string
	Tasks      []Task
	ChanCap    map[string]int
	Mutexes    []string
	Locs       []string
	NTasks     int
	SumChanCap int
	Tier       string
	Outside    []string // OUTSIDE_BOUND reason codes
}

// ChanBound is the pre-registered Tier-B channel-depth budget (8).
// It may be raised by the caller to report an extended-bound run; such a
// run is reported separately and never counted as a pre-registered pass.
var ChanBound = 8

// Outside marks the program as outside the admission bound.
func (p *Program) OutsideBound(code, detail string) {
	p.Outside = append(p.Outside, code+": "+detail)
}

type extractor struct {
	fset     *token.FileSet
	file     *ast.File
	consts   map[string]int64
	pkgVars  map[string]Desc
	funcs    map[string]*ast.FuncDecl
	refField map[string][]string // struct type -> reference-typed field names
	chanCap  map[string]int
	prog     *Program
	spawned  map[string]int // callee name -> task id (one task per spawn site)
	depth    int
}

// Extract parses an emitted Go file and builds the model.
func Extract(path string, sha string) (*Program, error) {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, path, nil, parser.ParseComments)
	if err != nil {
		return nil, err
	}
	e := &extractor{
		fset:     fset,
		file:     f,
		consts:   map[string]int64{},
		pkgVars:  map[string]Desc{},
		funcs:    map[string]*ast.FuncDecl{},
		refField: map[string][]string{},
		chanCap:  map[string]int{},
		spawned:  map[string]int{},
		prog: &Program{
			Name:     path,
			GoSHA:    sha,
			ChanCap:  map[string]int{},
			Tier:     "A",
		},
	}
	e.collectDecls()
	if err := e.build(); err != nil {
		return nil, err
	}
	return e.prog, nil
}

func (e *extractor) collectDecls() {
	for _, d := range e.file.Decls {
		switch decl := d.(type) {
		case *ast.FuncDecl:
			if decl.Recv == nil {
				e.funcs[decl.Name.Name] = decl
			}
		case *ast.GenDecl:
			switch decl.Tok {
			case token.CONST:
				for _, s := range decl.Specs {
					vs := s.(*ast.ValueSpec)
					for i, n := range vs.Names {
						if i < len(vs.Values) {
							if v, ok := e.constInt(vs.Values[i]); ok {
								e.consts[n.Name] = v
							}
						}
					}
				}
			case token.VAR:
				for _, s := range decl.Specs {
					vs := s.(*ast.ValueSpec)
					for i, n := range vs.Names {
						var t ast.Expr
						if vs.Type != nil {
							t = vs.Type
						} else if i < len(vs.Values) {
							t = nil
							// infer from value below
						}
						d := Desc{Kind: KindCopy, Name: n.Name}
						if t != nil {
							d = e.describeType(t, n.Name)
						} else if i < len(vs.Values) {
							d = e.describeExpr(vs.Values[i], nil)
							d.Name = n.Name
						}
						e.pkgVars[n.Name] = d
					}
				}
			case token.TYPE:
				for _, s := range decl.Specs {
					ts := s.(*ast.TypeSpec)
					if st, ok := ts.Type.(*ast.StructType); ok {
						var refs []string
						for _, fld := range st.Fields.List {
							if len(fld.Names) == 0 {
								continue
							}
							k := e.typeKind(fld.Type)
							if k == KindShared || k == KindChan || k == KindAtomic {
								refs = append(refs, fld.Names[0].Name)
							}
						}
						e.refField[ts.Name.Name] = refs
					}
				}
			}
		}
	}
	// channel capacities from make(chan ...) assignments anywhere
	ast.Inspect(e.file, func(n ast.Node) bool {
		as, ok := n.(*ast.AssignStmt)
		if !ok || len(as.Lhs) != 1 || len(as.Rhs) != 1 {
			return true
		}
		call, ok := as.Rhs[0].(*ast.CallExpr)
		if !ok || !isMakeChan(call) {
			return true
		}
		name := baseName(as.Lhs[0])
		if name == "" {
			return true
		}
		cap := 0
		if len(call.Args) >= 2 {
			if v, ok := e.constInt(call.Args[1]); ok {
				cap = int(v)
			}
		}
		e.chanCap[name] = cap
		return true
	})
}

func isMakeChan(call *ast.CallExpr) bool {
	id, ok := call.Fun.(*ast.Ident)
	if !ok || id.Name != "make" || len(call.Args) == 0 {
		return false
	}
	_, ok = call.Args[0].(*ast.ChanType)
	return ok
}

func (e *extractor) build() error {
	for k, v := range e.chanCap {
		e.prog.ChanCap[k] = v
	}
	mainFn, ok := e.funcs["main"]
	if !ok {
		return fmt.Errorf("no main function")
	}
	mainTask := &Task{ID: 0, Name: "main"}
	e.prog.Tasks = append(e.prog.Tasks, *mainTask)
	env := map[string]Desc{}
	for k, v := range e.pkgVars {
		env[k] = v
	}
	if err := e.walk(mainFn.Body.List, env, 0, 0); err != nil {
		return err
	}
	e.prog.NTasks = len(e.prog.Tasks) - 1 // spawned tasks; main is the coordinator
	for _, t := range e.prog.Tasks {
		for _, s := range t.Steps {
			switch s.Kind {
			case Send, Recv, Close:
				if _, ok := e.prog.ChanCap[s.Chan]; !ok {
					e.prog.ChanCap[s.Chan] = 0
				}
			}
		}
	}
	for _, c := range e.prog.ChanCap {
		e.prog.SumChanCap += c
	}
	seen := map[string]bool{}
	for _, t := range e.prog.Tasks {
		for _, s := range t.Steps {
			if s.Loc != "" && !seen[s.Loc] {
				seen[s.Loc] = true
				e.prog.Locs = append(e.prog.Locs, s.Loc)
			}
			if s.Mutex != "" {
				dup := false
				for _, m := range e.prog.Mutexes {
					if m == s.Mutex {
						dup = true
					}
				}
				if !dup {
					e.prog.Mutexes = append(e.prog.Mutexes, s.Mutex)
				}
			}
		}
	}
	// admission (measured, never clamped)
	if e.prog.NTasks > 4 {
		e.prog.OutsideBound("TASKS", fmt.Sprintf("spawned tasks=%d > 4", e.prog.NTasks))
	}
	if e.prog.SumChanCap > ChanBound {
		e.prog.OutsideBound("CHAN_DEPTH", fmt.Sprintf("sum(chan cap)=%d > %d", e.prog.SumChanCap, ChanBound))
	}
	if e.prog.SumChanCap <= 2 {
		e.prog.Tier = "A"
	} else {
		e.prog.Tier = "B"
	}
	return nil
}

func (e *extractor) walk(stmts []ast.Stmt, env map[string]Desc, taskID, depth int) error {
	if depth > 8 {
		e.prog.OutsideBound("UNSUPPORTED", "inlining depth > 8")
		return nil
	}
	for _, st := range stmts {
		if err := e.stmt(st, env, taskID, depth); err != nil {
			return err
		}
	}
	return nil
}

func (e *extractor) add(taskID int, s Step) {
	e.prog.Tasks[taskID].Steps = append(e.prog.Tasks[taskID].Steps, s)
}

func (e *extractor) stmt(st ast.Stmt, env map[string]Desc, taskID, depth int) error {
	switch s := st.(type) {
	case *ast.BlockStmt:
		return e.walk(s.List, env, taskID, depth)

	case *ast.DeclStmt:
		if gd, ok := s.Decl.(*ast.GenDecl); ok {
			for _, sp := range gd.Specs {
				vs := sp.(*ast.ValueSpec)
				for i, n := range vs.Names {
					var d Desc
					if vs.Type != nil {
						d = e.describeType(vs.Type, n.Name)
					} else if i < len(vs.Values) {
						d = e.describeExpr(vs.Values[i], env)
						d.Name = n.Name
					}
					if d.Kind == KindShared && d.Name == n.Name {
						d.Name = fmt.Sprintf("t%d.%s", taskID, n.Name)
					}
					env[n.Name] = d
				}
			}
		}
		return nil

	case *ast.AssignStmt:
		if len(s.Rhs) == 1 {
			if u, ok := s.Rhs[0].(*ast.UnaryExpr); ok && u.Op == token.ARROW {
				ch := baseName(u.X)
				e.add(taskID, Step{Kind: Recv, Chan: ch, Label: "recv " + ch})
				return nil
			}
		}
		for _, lhs := range s.Lhs {
			if loc, ok := e.sharedLoc(lhs, env); ok {
				e.add(taskID, Step{Kind: Write, Loc: loc, Label: "write " + loc})
			}
		}
		// record inferred types for `:=` bindings
		if s.Tok == token.DEFINE {
			for i, lhs := range s.Lhs {
				if id, ok := lhs.(*ast.Ident); ok && i < len(s.Rhs) {
					d := e.describeExpr(s.Rhs[i], env)
					d.Name = id.Name
					if d.Kind == KindShared {
						d.Name = fmt.Sprintf("t%d.%s", taskID, id.Name)
					}
					env[id.Name] = d
				}
			}
		}
		e.scanReads(s.Rhs, env, taskID, true)
		return nil

	case *ast.IncDecStmt:
		if loc, ok := e.sharedLoc(s.X, env); ok {
			e.add(taskID, Step{Kind: Write, Loc: loc, Label: "write " + loc})
		}
		e.scanReads([]ast.Expr{s.X}, env, taskID, false)
		return nil

	case *ast.SendStmt:
		ch := baseName(s.Chan)
		e.add(taskID, Step{Kind: Send, Chan: ch, Label: "send " + ch})
		return nil

	case *ast.ExprStmt:
		call, ok := s.X.(*ast.CallExpr)
		if !ok {
			return nil
		}
		if id, ok := call.Fun.(*ast.Ident); ok && id.Name == "close" && len(call.Args) == 1 {
			e.add(taskID, Step{Kind: Close, Chan: baseName(call.Args[0]), Label: "close"})
			return nil
		}
		if sel, ok := call.Fun.(*ast.SelectorExpr); ok {
			switch sel.Sel.Name {
			case "Lock":
				e.add(taskID, Step{Kind: Lock, Mutex: selectorName(sel), Label: "lock"})
				return nil
			case "Unlock":
				e.add(taskID, Step{Kind: Unlock, Mutex: selectorName(sel), Label: "unlock"})
				return nil
			case "Add", "Load", "Store", "Swap", "CompareAndSwap":
				return nil // atomic: safe by type
			}
		}
		e.scanReads(call.Args, env, taskID, true)
		return nil

	case *ast.GoStmt:
		return e.spawn(s.Call, env, taskID, depth)

	case *ast.ForStmt:
		return e.forStmt(s, env, taskID, depth)

	case *ast.RangeStmt:
		return e.rangeStmt(s, env, taskID, depth)

	case *ast.IfStmt:
		// Constant-foldable condition: take that branch. Otherwise take
		// both branches in sequence (a sound over-approximation).
		if v, ok := e.constCond(s.Cond, env); ok {
			if v {
				return e.walk(s.Body.List, env, taskID, depth)
			}
			if s.Else != nil {
				if b, ok := s.Else.(*ast.BlockStmt); ok {
					return e.walk(b.List, env, taskID, depth)
				}
			}
			return nil
		}
		if err := e.walk(s.Body.List, env, taskID, depth); err != nil {
			return err
		}
		if s.Else != nil {
			if b, ok := s.Else.(*ast.BlockStmt); ok {
				return e.walk(b.List, env, taskID, depth)
			}
		}
		return nil

	case *ast.ReturnStmt:
		e.scanReads(s.Results, env, taskID, true)
		return nil

	case *ast.LabeledStmt:
		return e.stmt(s.Stmt, env, taskID, depth)
	}
	return nil
}

func (e *extractor) spawn(call *ast.CallExpr, env map[string]Desc, parent, depth int) error {
	fnName := baseName(call.Fun)
	fn, ok := e.funcs[fnName]
	if !ok {
		e.prog.OutsideBound("UNSUPPORTED", "spawn of unknown function "+fnName)
		return nil
	}
	tid := len(e.prog.Tasks)
	e.prog.Tasks = append(e.prog.Tasks, Task{ID: tid, Name: fnName})
	e.add(parent, Step{Kind: Spawn, Task: tid, Label: "spawn " + fnName})

	// bind callee params to actual arguments
	child := map[string]Desc{}
	for k, v := range e.pkgVars {
		child[k] = v
	}
	params := fn.Type.Params.List
	idx := 0
	for _, fld := range params {
		for _, n := range fld.Names {
			var d Desc
			if idx < len(call.Args) {
				d = e.describeExpr(call.Args[idx], env)
			} else {
				d = e.describeType(fld.Type, n.Name)
			}
			if d.Name == "" {
				d.Name = n.Name
			}
			child[n.Name] = d
			if v, ok := e.constInt(call.Args[idx]); ok {
				e.consts[n.Name] = v
			}
			idx++
		}
	}
	// The location identity must be the caller's root, not the callee's
	// parameter name, so two spawn sites that share a value share a loc.
	for _, fld := range params {
		for _, n := range fld.Names {
			d := child[n.Name]
			if d.Kind == KindShared && d.Name != n.Name {
				// keep caller root; already set
			}
		}
	}
	if fn.Body == nil {
		e.prog.OutsideBound("UNSUPPORTED", "spawn of bodyless function "+fnName)
		return nil
	}
	return e.walk(fn.Body.List, child, tid, depth+1)
}

func (e *extractor) forStmt(s *ast.ForStmt, env map[string]Desc, taskID, depth int) error {
	bound, ok := e.loopBound(s, env)
	if !ok {
		e.prog.OutsideBound("LOOP", "non-constant loop bound")
		return nil
	}
	sync := e.hasSync(s.Body.List, env)
	if !sync {
		return e.walk(s.Body.List, env, taskID, depth) // summarize
	}
	if bound > 8 {
		e.prog.OutsideBound("LOOP", fmt.Sprintf("synchronizing loop bound %d > 8", bound))
		return nil
	}
	for i := 0; i < bound; i++ {
		if err := e.walk(s.Body.List, env, taskID, depth); err != nil {
			return err
		}
	}
	return nil
}

func (e *extractor) rangeStmt(s *ast.RangeStmt, env map[string]Desc, taskID, depth int) error {
	d := e.describeExpr(s.X, env)
	if d.Kind == KindChan {
		cap := e.chanCap[d.Name]
		if cap == 0 || cap > 8 {
			e.prog.OutsideBound("LOOP", fmt.Sprintf("range over channel %s with cap %d", d.Name, cap))
			return nil
		}
		for i := 0; i < cap; i++ {
			e.add(taskID, Step{Kind: Recv, Chan: d.Name, Label: "recv " + d.Name})
			if err := e.walk(s.Body.List, env, taskID, depth); err != nil {
				return err
			}
		}
		return nil
	}
	if e.hasSync(s.Body.List, env) {
		if n, ok := e.constSliceLen(s.X); ok && n <= 8 {
			for i := 0; i < int(n); i++ {
				if err := e.walk(s.Body.List, env, taskID, depth); err != nil {
					return err
				}
			}
			return nil
		}
		e.prog.OutsideBound("LOOP", "synchronizing range over non-channel")
		return nil
	}
	return e.walk(s.Body.List, env, taskID, depth)
}

func (e *extractor) loopBound(s *ast.ForStmt, env map[string]Desc) (int, bool) {
	if s.Init == nil || s.Cond == nil || s.Post == nil {
		return 0, false
	}
	as, ok := s.Init.(*ast.AssignStmt)
	if !ok || len(as.Lhs) != 1 || len(as.Rhs) != 1 {
		return 0, false
	}
	id, ok := as.Lhs[0].(*ast.Ident)
	if !ok {
		return 0, false
	}
	start, ok := e.constInt(as.Rhs[0])
	if !ok {
		return 0, false
	}
	be, ok := s.Cond.(*ast.BinaryExpr)
	if !ok || be.Op != token.LSS {
		return 0, false
	}
	lid, ok := be.X.(*ast.Ident)
	if !ok || lid.Name != id.Name {
		return 0, false
	}
	end, ok := e.constInt(be.Y)
	if !ok {
		return 0, false
	}
	inc, ok := s.Post.(*ast.IncDecStmt)
	if !ok || inc.Tok != token.INC {
		return 0, false
	}
	iid, ok := inc.X.(*ast.Ident)
	if !ok || iid.Name != id.Name {
		return 0, false
	}
	if end <= start {
		return 0, true
	}
	return int(end - start), true
}

func (e *extractor) hasSync(stmts []ast.Stmt, env map[string]Desc) bool {
	found := false
	for _, st := range stmts {
		ast.Inspect(st, func(n ast.Node) bool {
			switch x := n.(type) {
			case *ast.SendStmt:
				found = true
			case *ast.UnaryExpr:
				if x.Op == token.ARROW {
					found = true
				}
			case *ast.GoStmt:
				found = true
			case *ast.SelectorExpr:
				if x.Sel.Name == "Lock" || x.Sel.Name == "Unlock" {
					found = true
				}
			}
			return !found
		})
	}
	return found
}

func (e *extractor) constCond(cond ast.Expr, env map[string]Desc) (bool, bool) {
	be, ok := cond.(*ast.BinaryExpr)
	if !ok {
		if id, ok := cond.(*ast.Ident); ok {
			if v, ok := e.consts[id.Name]; ok {
				return v != 0, true
			}
		}
		return false, false
	}
	lv, lok := e.constInt(be.X)
	rv, rok := e.constInt(be.Y)
	if !lok || !rok {
		return false, false
	}
	switch be.Op {
	case token.GTR:
		return lv > rv, true
	case token.LSS:
		return lv < rv, true
	case token.GEQ:
		return lv >= rv, true
	case token.LEQ:
		return lv <= rv, true
	case token.EQL:
		return lv == rv, true
	case token.NEQ:
		return lv != rv, true
	}
	return false, false
}

func (e *extractor) constInt(x ast.Expr) (int64, bool) {
	switch v := x.(type) {
	case *ast.BasicLit:
		if v.Kind == token.INT {
			n, err := strconv.ParseInt(v.Value, 0, 64)
			return n, err == nil
		}
	case *ast.Ident:
		if n, ok := e.consts[v.Name]; ok {
			return n, true
		}
	case *ast.CallExpr:
		if id, ok := v.Fun.(*ast.Ident); ok && id.Name == "len" && len(v.Args) == 1 {
			if c, ok := e.constSliceLen(v.Args[0]); ok {
				return c, true
			}
		}
	case *ast.ParenExpr:
		return e.constInt(v.X)
	}
	return 0, false
}

func (e *extractor) constSliceLen(x ast.Expr) (int64, bool) {
	id, ok := x.(*ast.Ident)
	if !ok {
		return 0, false
	}
	for _, d := range e.file.Decls {
		gd, ok := d.(*ast.GenDecl)
		if !ok || gd.Tok != token.VAR {
			continue
		}
		for _, sp := range gd.Specs {
			vs := sp.(*ast.ValueSpec)
			for i, n := range vs.Names {
				if n.Name != id.Name || i >= len(vs.Values) {
					continue
				}
				if cl, ok := vs.Values[i].(*ast.CompositeLit); ok {
					return int64(len(cl.Elts)), true
				}
			}
		}
	}
	return 0, false
}

func (e *extractor) scanReads(exprs []ast.Expr, env map[string]Desc, taskID int, include bool) {
	for _, ex := range exprs {
		ast.Inspect(ex, func(n ast.Node) bool {
			id, ok := n.(*ast.Ident)
			if !ok {
				return true
			}
			if d, ok := env[id.Name]; ok && d.Kind == KindShared {
				e.add(taskID, Step{Kind: Read, Loc: d.Name, Label: "read " + d.Name})
			}
			return true
		})
	}
}

func (e *extractor) sharedLoc(x ast.Expr, env map[string]Desc) (string, bool) {
	name := baseName(x)
	if name == "" {
		return "", false
	}
	if d, ok := env[name]; ok && d.Kind == KindShared {
		return d.Name, true
	}
	return "", false
}

func (e *extractor) describeType(t ast.Expr, name string) Desc {
	d := Desc{Name: name, Kind: KindCopy}
	switch tt := t.(type) {
	case *ast.ChanType:
		d.Kind = KindChan
		d.Cap = e.chanCap[name]
	case *ast.MapType, *ast.ArrayType:
		d.Kind = KindShared
	case *ast.StarExpr:
		if st, ok := tt.X.(*ast.SelectorExpr); ok && baseName(st) == "atomic" {
			d.Kind = KindAtomic
			return d
		}
		if st, ok := tt.X.(*ast.Ident); ok {
			d.Kind = KindShared
			if refs, ok := e.refField[st.Name]; ok {
				d.Fields = refs
			}
		}
	case *ast.Ident:
		if refs, ok := e.refField[tt.Name]; ok {
			d.Kind = KindShared
			d.Fields = refs
		}
	case *ast.SelectorExpr:
		if baseName(tt) == "atomic" {
			d.Kind = KindAtomic
		}
	case *ast.InterfaceType:
		d.Kind = KindUnknown
	}
	return d
}

func (e *extractor) describeExpr(x ast.Expr, env map[string]Desc) Desc {
	switch v := x.(type) {
	case *ast.UnaryExpr:
		if v.Op == token.AND {
			inner := e.describeExpr(v.X, env)
			inner.Kind = KindShared
			if inner.Name == "" {
				inner.Name = baseName(v.X)
			}
			return inner
		}
		if v.Op == token.ARROW {
			return Desc{Kind: KindCopy}
		}
	case *ast.CallExpr:
		if isMakeChan(v) {
			cap := 0
			if len(v.Args) >= 2 {
				if c, ok := e.constInt(v.Args[1]); ok {
					cap = int(c)
				}
			}
			return Desc{Kind: KindChan, Name: baseName(x), Cap: cap}
		}
		if id, ok := v.Fun.(*ast.Ident); ok {
			if fn, ok := e.funcs[id.Name]; ok {
				if fn.Type.Results != nil && len(fn.Type.Results.List) > 0 {
					return e.describeType(fn.Type.Results.List[0].Type, baseName(x))
				}
			}
		}
	case *ast.CompositeLit:
		if id, ok := v.Type.(*ast.Ident); ok {
			if refs, ok := e.refField[id.Name]; ok {
				return Desc{Kind: KindShared, Name: baseName(x), Fields: refs}
			}
			return Desc{Kind: KindCopy, Name: baseName(x)}
		}
		if _, ok := v.Type.(*ast.MapType); ok {
			return Desc{Kind: KindShared, Name: baseName(x)}
		}
		if _, ok := v.Type.(*ast.ArrayType); ok {
			return Desc{Kind: KindShared, Name: baseName(x)}
		}
	}
	name := baseName(x)
	if name == "" {
		return Desc{Kind: KindCopy}
	}
	if d, ok := env[name]; ok {
		return d
	}
	if d, ok := e.pkgVars[name]; ok {
		return d
	}
	return Desc{Kind: KindCopy, Name: name}
}

func (e *extractor) typeKind(t ast.Expr) Kind {
	d := e.describeType(t, "")
	return d.Kind
}

func baseName(x ast.Expr) string {
	switch v := x.(type) {
	case *ast.Ident:
		return v.Name
	case *ast.SelectorExpr:
		return baseName(v.X)
	case *ast.IndexExpr:
		return baseName(v.X)
	case *ast.StarExpr:
		return baseName(v.X)
	case *ast.ParenExpr:
		return baseName(v.X)
	case *ast.CallExpr:
		return baseName(v.Fun)
	}
	return ""
}

func selectorName(sel *ast.SelectorExpr) string {
	b := baseName(sel.X)
	if b == "" {
		return sel.Sel.Name
	}
	return b + "." + sel.Sel.Name
}

// ModelHash is a stable digest of the extracted model (tasks and steps).
func (p *Program) ModelHash() string {
	var b strings.Builder
	for _, t := range p.Tasks {
		fmt.Fprintf(&b, "T%d:%s|", t.ID, t.Name)
		for _, s := range t.Steps {
			fmt.Fprintf(&b, "%d,%d,%s,%s,%s,%s;", s.Kind, s.Task, s.Chan, s.Mutex, s.Loc, s.Label)
		}
		b.WriteString("\n")
	}
	return sha256Hex([]byte(b.String()))
}
