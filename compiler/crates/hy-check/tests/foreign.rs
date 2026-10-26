//! Tests for foreign type handling.

use hy_check::foreign::foreign_named_type;
use hy_check::model::TypeShape;

#[test]
fn test_sync_atomic_builtin() {
    let name = "sync/atomic.Int64";
    let shape = foreign_named_type(name);
    assert!(matches!(shape, Some(TypeShape::ForeignOpaque(ref s)) if s == name));
}
