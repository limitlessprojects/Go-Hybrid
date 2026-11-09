// 05-json-transform — stresses: calling a PINNED RUST CRATE (serde_json) from
// Hybrid through the checked boundary; string/slice views crossing without
// deep copies. TARGET SYNTAX.

import "rust/serde_json"   // pinned crate; boundary types are checked

fn main() {
    // JSON in (string view crosses the boundary as ptr+len, no copy)
    doc := serde_json.parse(`{"users":[{"name":"ada","logins":5},{"name":"linus","logins":2}]}`)?

    // transform in Hybrid: filter + map
    var names []string
    for u in doc.array("users")? {
        if u.int("logins")? >= 3 {
            names = append(names, u.string("name")?)
        }
    }

    // JSON out (Rust serializes; error is a Hybrid error value)
    out := serde_json.to_string_pretty(users_json(names))?
    fmt.println(out)
}

fn users_json(names []string) serde_json.value {
    v := serde_json.new_object()
    let arr := serde_json.new_array()
    for n in names {
        arr.push_string(n)
    }
    v.set("active", arr)
    return v
}
