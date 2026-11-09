// 02-cli-cat — stresses: CLI args, file I/O, error handling with `?`, defer.
// TARGET SYNTAX. Usage: program.hy in.txt ; exit code 1 + message on missing file.

fn main() (code int) {
    args := os.args()                    // no arg => usage
    if len(args) < 2 {
        fmt.eprintln("usage: cat <file>")
        return 1
    }

    f := os.open(args[1])?               // `?` propagates (T, error) upward
    defer f.close()

    data := io.read_all(f)?              // error path: exactly one allocation
    fmt.print(data)
    return 0
}
