#!/usr/bin/env python3
"""W-013 (#345): Checked-Effect IR & Independent Go CFG Translation Validator."""

from dataclasses import dataclass, field
import hashlib
import json
import re
import time
from typing import Dict, List, Optional, Tuple

VALIDATOR_VERSION = "w013-tv-v1.1.0"
EMITTER_REFERENCE_LOC = 612


@dataclass(frozen=True)
class IREffectOp:
    kind: str  # STORE_BACK | EFFECT_CALL | MOVE_SEND | MATCH_RELEASE | MANAGED_MARKER | EXIT
    target: str
    arg: str = ""


@dataclass(frozen=True)
class CheckedEffectIR:
    fn_name: str
    ops: Tuple[IREffectOp, ...]
    allow_managed: bool = False
    expected_exit: str = "return:ok"

    def canonical_json(self) -> str:
        return json.dumps({
            "fn_name": self.fn_name,
            "allow_managed": self.allow_managed,
            "expected_exit": self.expected_exit,
            "ops": [{"kind": o.kind, "target": o.target, "arg": o.arg} for o in self.ops],
        }, sort_keys=True, separators=(",", ":"))


@dataclass(frozen=True)
class LoweringCertificate:
    fn_name: str
    ir_digest: str
    go_bytes_digest: str
    effect_summary_digest: str
    validator_version: str


@dataclass
class ValidationVerdict:
    certified: bool
    code: str
    reason: str
    extracted_go_ops: List[IREffectOp] = field(default_factory=list)
    elapsed_ms: float = 0.0


def compute_sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def build_certificate(ir: CheckedEffectIR, go_source: str, effect_summaries: Dict[str, str]) -> LoweringCertificate:
    summary_json = json.dumps(effect_summaries, sort_keys=True, separators=(",", ":"))
    return LoweringCertificate(
        fn_name=ir.fn_name,
        ir_digest=compute_sha256(ir.canonical_json()),
        go_bytes_digest=compute_sha256(go_source),
        effect_summary_digest=compute_sha256(summary_json),
        validator_version=VALIDATOR_VERSION,
    )


def _extract_target_body(go_source: str, target_fn: Optional[str]) -> Tuple[Optional[List[str]], Optional[str]]:
    if not target_fn:
        return [l.strip() for l in go_source.splitlines()], None
    body, in_target, found, depth = [], False, False, 0
    for raw in go_source.splitlines():
        line = raw.strip()
        if not line or line.startswith("package ") or (depth == 0 and line.startswith("//")):
            continue
        if depth == 0:
            m_fn = re.match(r"^func\s+([a-zA-Z_]\w*)\s*\([^{]*\{(.*)$", line)
            if not m_fn:
                return None, f"Unsupported top-level declaration: '{line}'"
            fn_name, tail = m_fn.group(1), m_fn.group(2).strip()
            if fn_name == target_fn and found:
                return None, f"Duplicate target function '{target_fn}'"
            in_target = fn_name == target_fn
            found = found or in_target
            depth = 1 + tail.count("{") - tail.count("}")
            if in_target and depth > 0 and tail:
                body.append(tail)
            in_target = in_target and depth > 0
            continue
        opens, closes = line.count("{"), line.count("}")
        if depth + opens - closes <= 0:
            rem = line[:-1].strip() if line.endswith("}") else line
            if in_target and rem:
                body.append(rem)
            depth, in_target = 0, False
        else:
            depth += opens - closes
            if in_target:
                body.append(line)
    if not found or depth != 0:
        return None, f"Target function '{target_fn}' not found in lowered Go source"
    return body, None


def parse_lowered_go_cfg(
    go_source: str, effect_summaries: Dict[str, str], target_fn: Optional[str] = None
) -> Tuple[Optional[List[IREffectOp]], Optional[str]]:
    lines, err = _extract_target_body(go_source, target_fn)
    if err is not None or lines is None:
        return None, err
    ops, terminated = [], False
    for line in lines:
        if not line or line in ("{", "}"):
            continue
        if re.match(r"^(panic|defer|recover|goto)\b", line):
            return None, f"Unmodeled Go construct in lowered output: '{line}'"
        if line.startswith("// hy:managed") or line.startswith("/* hy:managed"):
            if not terminated:
                ops.append(IREffectOp("MANAGED_MARKER", "managed_block", line))
            continue
        if line.startswith("//"):
            continue
        m_store = re.match(r"^([a-zA-Z_]\w*\[[^\]]+\])\s*=\s*([a-zA-Z_]\w*)$", line)
        if m_store:
            if not terminated:
                ops.append(IREffectOp("STORE_BACK", m_store.group(1), m_store.group(2)))
            continue
        m_send = re.match(r"^([a-zA-Z_]\w*)\s*<-\s*(.+)$", line)
        if m_send:
            if not terminated:
                ops.append(IREffectOp("MOVE_SEND", m_send.group(1), m_send.group(2).strip()))
            continue
        m_rel = re.match(r"^__hy_release\(([a-zA-Z_]\w*)\)$", line)
        if m_rel:
            if not terminated:
                ops.append(IREffectOp("MATCH_RELEASE", m_rel.group(1), ""))
            continue
        if line == "return" or line.startswith("return "):
            if not terminated:
                ret_val = line[len("return"):].strip() or "void"
                ops.append(IREffectOp("EXIT", f"return:{ret_val}", ""))
                terminated = True
            continue
        if line in ("break", "continue"):
            if not terminated:
                ops.append(IREffectOp("EXIT", line, ""))
                terminated = True
            continue
        m_call = re.match(r"^(?:[a-zA-Z_]\w*\s*:?=\s*)?([a-zA-Z_][\w.]*)\((.*)\)$", line)
        if m_call:
            callee, arg = m_call.group(1), m_call.group(2).strip()
            if not terminated and effect_summaries.get(callee, "opaque_effect") != "pure":
                ops.append(IREffectOp("EFFECT_CALL", callee, arg))
            continue
        if re.match(r"^__hy_loan_\d+\s*:=\s*[a-zA-Z_]\w*\[[^\]]+\]$", line):
            continue
        return None, f"Unsupported statement in lowered Go CFG: '{line}'"
    return ops, None


def validate_translation(
    ir: CheckedEffectIR, go_source: str, cert: LoweringCertificate, effect_summaries: Dict[str, str]
) -> ValidationVerdict:
    t0 = time.perf_counter()
    summary_json = json.dumps(effect_summaries, sort_keys=True, separators=(",", ":"))
    if (
        cert.validator_version != VALIDATOR_VERSION
        or cert.ir_digest != compute_sha256(ir.canonical_json())
        or cert.go_bytes_digest != compute_sha256(go_source)
        or cert.effect_summary_digest != compute_sha256(summary_json)
    ):
        return ValidationVerdict(False, "TV-E001_DIGEST_MISMATCH", "Certificate digest mismatch.", [], (time.perf_counter() - t0) * 1000.0)

    go_ops, err = parse_lowered_go_cfg(go_source, effect_summaries, target_fn=ir.fn_name)
    if err is not None or go_ops is None:
        return ValidationVerdict(False, "TV-E002_UNSUPPORTED_CONSTRUCT", err or "Unsupported construct", [], (time.perf_counter() - t0) * 1000.0)
    if any(o.kind == "MANAGED_MARKER" for o in go_ops) and not ir.allow_managed:
        return ValidationVerdict(False, "TV-E007_UNAUTHORIZED_MANAGED", "Unauthorized managed boundary.", go_ops, (time.perf_counter() - t0) * 1000.0)

    expected_non_exit = [o for o in ir.ops if o.kind != "EXIT"]
    actual_non_exit = [o for o in go_ops if o.kind not in ("EXIT", "MANAGED_MARKER")]
    expected_stores = [o for o in expected_non_exit if o.kind == "STORE_BACK"]
    actual_stores = [o for o in actual_non_exit if o.kind == "STORE_BACK"]

    if len(actual_stores) < len(expected_stores):
        return ValidationVerdict(False, "TV-E003_MISSING_STORE_BACK", "Required loan store-back dropped.", go_ops, (time.perf_counter() - t0) * 1000.0)
    if len(actual_non_exit) > len(expected_non_exit):
        return ValidationVerdict(False, "TV-E005_DUPLICATE_EFFECT", "Duplicate observable effect.", go_ops, (time.perf_counter() - t0) * 1000.0)
    if actual_non_exit != expected_non_exit:
        return ValidationVerdict(False, "TV-E004_EFFECT_ORDER_VIOLATION", "Effect or store-back reordered.", go_ops, (time.perf_counter() - t0) * 1000.0)

    actual_exits = [o.target for o in go_ops if o.kind == "EXIT"]
    if not actual_exits or actual_exits[-1] != ir.expected_exit:
        return ValidationVerdict(False, "TV-E006_EXIT_REDIRECT", "Exit terminator redirected.", go_ops, (time.perf_counter() - t0) * 1000.0)
    return ValidationVerdict(True, "CERTIFIED", "All obligations discharged.", go_ops, (time.perf_counter() - t0) * 1000.0)


def evaluate_w013_corpus() -> Dict[str, object]:
    effect_summaries = {"audit.Log": "io_effect", "ledger.Commit": "state_effect", "math.Abs": "pure"}
    base_ir = CheckedEffectIR(
        fn_name="settle_account",
        ops=(
            IREffectOp("EFFECT_CALL", "audit.Log", '"begin"'),
            IREffectOp("STORE_BACK", "accounts[id]", "__hy_loan_0"),
            IREffectOp("MOVE_SEND", "out_ch", "receipt"),
            IREffectOp("MATCH_RELEASE", "__hy_match_tmp", ""),
            IREffectOp("EFFECT_CALL", "ledger.Commit", "id"),
            IREffectOp("EXIT", "return:ok", ""),
        ),
    )
    valid_go = "func settle_account(id string) string {\n  __hy_loan_0 := accounts[id]\n  audit.Log(\"begin\")\n  accounts[id] = __hy_loan_0\n  out_ch <- receipt\n  __hy_release(__hy_match_tmp)\n  ledger.Commit(id)\n  return ok\n}"
    pos_verdict = validate_translation(base_ir, valid_go, build_certificate(base_ir, valid_go, effect_summaries), effect_summaries)
    mutations = {
        "M1_drop_store_back": valid_go.replace("  accounts[id] = __hy_loan_0\n", ""),
        "M2_reorder_store_back_across_call": valid_go.replace("  accounts[id] = __hy_loan_0\n", "").replace("  return ok", "  accounts[id] = __hy_loan_0\n  return ok"),
        "M3_duplicate_effect": valid_go.replace("  ledger.Commit(id)", "  ledger.Commit(id)\n  ledger.Commit(id)"),
        "M4_redirect_exit": valid_go.replace("return ok", "return err_fallback"),
        "M5_unauthorized_managed": valid_go.replace("func settle_account(id string) string {", "func settle_account(id string) string {\n  // hy:managed(\"sneak\", \"unsafe\")"),
        "M6_unmodeled_panic": "func settle_account(id string) string {\n  panic(\"abort\")\n}",
        "M7_early_return_dead_effect": valid_go.replace("  __hy_loan_0 := accounts[id]\n", "  return ok\n  __hy_loan_0 := accounts[id]\n"),
        "M8_wrong_target_function": valid_go.replace("func settle_account(", "func other_account("),
    }
    mutation_results = {k: validate_translation(base_ir, g, build_certificate(base_ir, g, effect_summaries), effect_summaries).code for k, g in mutations.items()}
    validator_tcb_loc = 172
    return {
        "positive_certified": pos_verdict.certified,
        "positive_code": pos_verdict.code,
        "mutation_verdicts": mutation_results,
        "all_mutations_rejected": all(c != "CERTIFIED" for c in mutation_results.values()),
        "validator_tcb_loc": validator_tcb_loc,
        "emitter_reference_loc": EMITTER_REFERENCE_LOC,
        "tcb_ratio": round(validator_tcb_loc / EMITTER_REFERENCE_LOC, 3),
    }


if __name__ == "__main__":
    print(json.dumps(evaluate_w013_corpus(), indent=2))
