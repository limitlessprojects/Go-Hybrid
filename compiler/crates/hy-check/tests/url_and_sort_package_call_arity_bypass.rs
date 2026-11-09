//! Regression tests for foreign package function calls (`url.*`, `sort.*`).
//! Enforces call arity constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).

use hy_check::{check_file, CheckResult };\nuse+(y"...