package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func checkSource(t *testing.T, source string) error {
	t.Helper()
	file := filepath.Join(t.TempDir(), "input.go")
	if err := os.WriteFile(file, []byte(source), 0600); err != nil {
		t.Fatal(err)
	}
	return check([]string{"-p", "main", file})
}
func TestAdmittedNativeSubset(t *testing.T) {
	if err := checkSource(t, `package main
//hybrid:checked
func worker(x int,done chan int){done<-x}
//hybrid:checked
func start(x int,done chan int){go worker(x,done)}
func main(){}`); err != nil {
		t.Fatal(err)
	}
}
func TestRejectUnprovedOwnershipAndEffects(t *testing.T) {
	cases := []string{
		`//hybrid:checked
func checked(x []int){x[0]=1}`,
		`var global int
//hybrid:checked
func checked(){global++}`,
		`func other(){}
//hybrid:checked
func checked(){other()}`,
		`//hybrid:checked
func checked(x int){p:=&x;_ = p}`,
		`//hybrid:checked
func checked(){go func(){}()}`,
		`//hybrid:checked
func checked(x chan []int){_ = x}`,
	}
	for _, source := range cases {
		if err := checkSource(t, "package main\n"+source+"\nfunc main(){}\n"); err == nil {
			t.Fatal("unproved input admitted:", source)
		}
	}
}
func TestMalformedMarkerFailsClosed(t *testing.T) {
	err := checkSource(t, "package main\n//hybrid:checked\nvar x int\nfunc main(){}")
	if err == nil || !strings.Contains(err.Error(), "annotation") {
		t.Fatal(err)
	}
}
func TestOrdinaryGoIsPassthrough(t *testing.T) {
	if err := checkSource(t, "package main\nfunc main(){x:=map[int]int{};x[1]=2}"); err != nil {
		t.Fatal(err)
	}
}

func TestMarkerInsideStringDoesNotOptIn(t *testing.T) {
	if err := checkSource(t, "package main\nconst text=\"//hybrid:checked\"\nfunc main(){}"); err != nil {
		t.Fatal(err)
	}
}

func TestRPCFailureIsNotNativeApproval(t *testing.T) {
	if err := checkSource(t, `package main
import "fmt"
//hybrid:checked
func checked(){fmt.Println("foreign")}
func main(){}`); err == nil {
		t.Fatal("missing export data accepted")
	}
}
