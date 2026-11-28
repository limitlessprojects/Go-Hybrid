"""W-4723 (#4723): Constant conversion overflow check for narrower integer types.

Validates that constant conversions to narrower integer types (int8, int16, etc.)
are rejected when the constant value overflows the target type, matching Go's behavior.
"""

from __future__ import annotations
from dataclasses import dataclass
from enum import Enum
from typing import Optional, Tuple


class IntType(str, Enum):
    INT8 = "int8"
    INT16 = "int16"
    INT32 = "int32"
    INT64 = "int64"
    UINT8 = "uint8"
    UINT16 = "uint16"
    UINT32 = "uint32"
    UINT64 = "uint64"


# Ranges for each integer type
INT_RANGES = {
    IntType.INT8:   (-128, 127),
    IntType.INT16:  (-32768, 32767),
    IntType.INT32:  (-2147483648, 2147483647),
    IntType.INT64:  (-9223372036854775808, 9223372036854775807),
    IntType.UINT8:  (0, 255),
    IntType.UINT16: (0, 65535),
    IntType.UINT32: (0, 4294967295),
    IntType.UINT64: (0, 18446744073709551615),
}


@dataclass(frozen=True)
class ConstConversionResult:
    accepted: bool
    diagnostic: Optional[str] = None
    code: Optional[str] = None


def check_integer_constant_conversion(value: int, target_type: IntType) -> ConstConversionResult:
    """Checks whether an integer constant fits the target integer type.

    Go rejects constant conversions that overflow ANY integer width at compile time.
    The current hy-check only validates 64-bit targets, silently folding narrower types.
    This fix applies the overflow rule uniformly across all integer widths.
    """
    lo, hi = INT_RANGES[target_type]

    if value < lo or value > hi:
        return ConstConversionResult(
            accepted=False,
            diagnostic=f"E6006: constant {value} overflows {target_type.value}",
            code="E6006",
        )

    return ConstConversionResult(accepted=True)


def check_float_constant_conversion(value: float, target_type: IntType) -> ConstConversionResult:
    """Checks whether a float constant can be represented as the target integer type.

    Float constants must be exact integers and within range. Go rejects truncation.
    """
    if value != int(value):
        return ConstConversionResult(
            accepted=False,
            diagnostic=f"E6006: constant {value} is not representable as {target_type.value} (truncated)",
            code="E6006",
        )

    return check_integer_constant_conversion(int(value), target_type)


# Batch validator matching the differential test table from the issue
def validate_conversion_table() -> list[Tuple[str, bool, Optional[str]]]:
    """Runs the exact test matrix from issue #4723 and returns results."""
    results = []

    cases = [
        ("int8(200)", 200, IntType.INT8, False),
        ("int8(-200)", -200, IntType.INT8, False),
        ("uint64(-1)", -1, IntType.UINT64, False),
        ("int64(9999999999999999999)", 9999999999999999999, IntType.INT64, False),
        ("int64(9223372036854775807)", 9223372036854775807, IntType.INT64, True),  # max int64
        ("int8(127)", 127, IntType.INT8, True),
        ("int8(-128)", -128, IntType.INT8, True),
        ("uint8(255)", 255, IntType.UINT8, True),
        ("uint8(256)", 256, IntType.UINT8, False),
        ("int16(32768)", 32768, IntType.INT16, False),
        ("int16(32767)", 32767, IntType.INT16, True),
    ]

    for label, value, target, expected_ok in cases:
        res = check_integer_constant_conversion(value, target)
        match = res.accepted == expected_ok
        results.append((label, match, res.diagnostic))

    return results
