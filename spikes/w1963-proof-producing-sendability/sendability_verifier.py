"""W-1963 (#1963): Independent Proof-Carrying Sendability Verifier for hy-check."""

from __future__ import annotations
import hashlib
from dataclasses import dataclass
from typing import Dict, List, Tuple

PRIMITIVES = {"int", "int64", "uint64", "float64", "bool", "string"}
SEMANTICS_VERSION = "hy-sendability-v1"


@dataclass(frozen=True)
class BindingDecl:
    binding_id: str
    name: str
    ty: str
    scope_depth: int = 0
    aliased_before_spawn: bool = False
    used_after_spawn: bool = False


@dataclass(frozen=True)
class SendabilityCertificate:
    semantics_version: str
    source_sha256: str
    import_closure_sha256: str
    spawn_arg_name: str
    claimed_binding_id: str
    rule: str  # "RULE_PRIM_COPY" | "RULE_VALUE_STRUCT_CLOSURE" | "RULE_UNIQUE_FRESH_MOVE"
    claimed_struct_fields: Tuple[Tuple[str, str], ...] = ()
    has_explicit_move: bool = False


@dataclass(frozen=True)
class VerificationVerdict:
    certified: bool
    code: str
    reason: str


def compute_digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def verify_sendability_proof(
    source_text: str,
    import_closure_text: str,
    decls: List[BindingDecl],
    struct_defs: Dict[str, Dict[str, str]],
    cert: SendabilityCertificate,
) -> VerificationVerdict:
    """Independently replay a candidate sendability derivation against source + type closure."""
    if cert.semantics_version != SEMANTICS_VERSION:
        return VerificationVerdict(False, "W1963_ERR_SEMANTICS_VERSION", "unsupported semantics version")
    if cert.source_sha256 != compute_digest(source_text):
        return VerificationVerdict(False, "W1963_ERR_SOURCE_DIGEST_MISMATCH", "source hash mismatch")
    if cert.import_closure_sha256 != compute_digest(import_closure_text):
        return VerificationVerdict(False, "W1963_ERR_IMPORT_DIGEST_MISMATCH", "import closure hash mismatch")

    # Resolve innermost visible binding for spawn_arg_name from source declarations
    candidates = [d for d in decls if d.name == cert.spawn_arg_name]
    if not candidates:
        return VerificationVerdict(False, "W1963_ERR_UNBOUND_ARG", f"unknown arg '{cert.spawn_arg_name}'")
    actual = max(candidates, key=lambda d: d.scope_depth)
    if actual.binding_id != cert.claimed_binding_id:
        return VerificationVerdict(
            False,
            "W1963_ERR_SHADOWED_BINDING_ID",
            f"certificate claims binding '{cert.claimed_binding_id}' but innermost is '{actual.binding_id}'",
        )

    if cert.rule == "RULE_PRIM_COPY":
        if actual.ty not in PRIMITIVES:
            return VerificationVerdict(
                False, "W1963_ERR_FORGED_PRIMITIVE", f"type '{actual.ty}' is not in PRIMITIVES"
            )
        return VerificationVerdict(True, "CERTIFIED_PRIM_COPY", "primitive copy sendability verified")

    if cert.rule == "RULE_VALUE_STRUCT_CLOSURE":
        if actual.ty not in struct_defs:
            return VerificationVerdict(False, "W1963_ERR_UNKNOWN_STRUCT", f"unknown struct '{actual.ty}'")
        real_fields = struct_defs[actual.ty]
        claimed_map = dict(cert.claimed_struct_fields)
        if set(real_fields.keys()) != set(claimed_map.keys()):
            return VerificationVerdict(
                False, "W1963_ERR_OMITTED_STRUCT_FIELD", "certificate omitted or altered struct fields"
            )
        for fname, fty in real_fields.items():
            if claimed_map.get(fname) != fty or fty not in PRIMITIVES:
                return VerificationVerdict(
                    False,
                    "W1963_ERR_NON_VALUE_FIELD",
                    f"field '{fname}: {fty}' is not pointer-free primitive",
                )
        return VerificationVerdict(True, "CERTIFIED_VALUE_STRUCT", "pointer-free struct sendability verified")

    if cert.rule == "RULE_UNIQUE_FRESH_MOVE":
        if not cert.has_explicit_move:
            return VerificationVerdict(False, "W1963_ERR_MISSING_MOVE", "linear move witness missing")
        if actual.aliased_before_spawn or actual.used_after_spawn:
            return VerificationVerdict(
                False, "W1963_ERR_LINEARITY_VIOLATION", "binding aliased before or used after task spawn"
            )
        return VerificationVerdict(True, "CERTIFIED_UNIQUE_MOVE", "unique fresh move sendability verified")

    return VerificationVerdict(False, "W1963_ERR_UNKNOWN_RULE", f"unrecognized rule '{cert.rule}'")
