package listedwildcardprobe

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// Detached phantom witnesses carry no relationship to the emitted operations.
// Only compile these mutations; several are intentionally race-inducing.
func TestWitnessMutationMatrix(t *testing.T) {
	families := []struct{ name, body string }{
		{"dropped-storeback", `c:=cell{};v:=c.n;v=%d;_ = v;return c.n`},
		{"pointer-instead-of-copy", `c:=&cell{};alias:=c;alias.n=%d;return c.n`},
		{"shared-task-capture", `c:=&cell{};go func(){c.n=%d}();return c.n`},
		{"reordered-channel-sends", `ch:=make(chan int,2);ch<-%d+1;ch<-0;return <-ch`},
		{"move-source-reuse", `x:=Own[int]{Value:%d};a:=move(x);b:=move(x);return a.Value+b.Value`},
	}
	compiled := 0
	for _, family := range families {
		for i := 1; i <= 10; i++ {
			dir := t.TempDir()
			source := `package main
 type cell struct{n int}
 type _witness_O8 [0]func()
 var _ = _witness_O8{}
 type Own[T any] struct{Value T;witness [0]func(T)}
 func move[T any](x Own[T])Own[T]{return x}
 func observed()int{` + fmt.Sprintf(family.body, i) + `}
 func main(){_ = observed()}
 `
			input := filepath.Join(dir, "mutant.go")
			if err := os.WriteFile(input, []byte(source), 0600); err != nil {
				t.Fatal(err)
			}
			cmd := exec.Command("go", "build", "-o", filepath.Join(dir, "mutant.exe"), input)
			output, err := cmd.CombinedOutput()
			if err != nil {
				t.Fatalf("unexpected syntax/tool failure for %s/%d: %v: %s", family.name, i, err, output)
			}
			compiled++
			t.Logf("%s/%02d: COMPILED (proposed witness did not reject mutation)", family.name, i)
		}
	}
	if compiled != 50 {
		t.Fatal("incomplete mutation matrix")
	}
	t.Logf("proposal criterion: reject 50/50; observed: rejected 0/50, compiled %d/50; detached-witness sufficiency FALSIFIED", compiled)
}
