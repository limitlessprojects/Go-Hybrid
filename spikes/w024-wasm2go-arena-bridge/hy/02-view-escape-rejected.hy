// W-024 (Issue #757) sample 02: REJECT (HYB-E3007) — BorrowHandle (`serde_json.view`)
// escaping its owning `OwnHandle` (`serde_json.doc`) across function return is
// rejected statically by `hy-check` (and trapped at runtime by the arena generation check).
package main

import "rust/serde_json"

fn leak_view(raw string) (serde_json.view, error):
    managed("attempt to return borrowed arena view", "w024-spike"):
        doc := serde_json.parse(raw)?
        defer doc.free()
        return doc.root(), nil
