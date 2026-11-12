import "flag"

struct box[t] {
    val: t
}

fn parse_verbose(fs: *flag.FlagSet) -> Result[int, error] {
    let m: map[string]int = {"ok": 1}
    let slot = &m["ok"]
    *slot = *slot + 1
    fs.bool_func("v", "verbose", nil)
    return Ok(m["ok"])
}
