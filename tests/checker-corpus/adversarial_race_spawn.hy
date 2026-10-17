// EXPECTED-VERDICT: REJECT
// Task spawn boundary soundness test: attempting to share a mutable reference across task boundary without managed/move

func main() {
    var x: int = 42
    spawn fn() {
        x = 100 // Unsafe shared mutable capture
    }()
    print(x)
}
