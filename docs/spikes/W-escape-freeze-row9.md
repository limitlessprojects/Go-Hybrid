# Spike `W-4565`: Dual-Polarity Foreign Escape Freeze (`O5.3` / `HYB-E3014`) to Flip `GUARANTEES.md` Row 9 (`#4565`)

- **Issue:** `#4565` (`[wildcard] Dual-Polarity Foreign Escape Freeze (O5.3 / HYB-E3014): Flip GUARANTEES.md Row 9 from Read-Only Escape Exemption to Full Tier-0 Guarantee`)
- **Track:** Wildcard Track (`$500` USD — `MOONSHOT.md` Prompt #2 & Prompt #4)
- **Spike directory:** `spikes/w-escape-freeze-row9/`
- **Verifier CLI:** `python3 spikes/w-escape-freeze-row9/escape_freeze_verifier.py --check`
- **Test suite:** `pytest spikes/w-escape-freeze-row9/tests/test_escape_freeze_row9.py`
- **Summary:** Closes the `GUARANTEES.md` Row 9 half-claim (*"Guaranteed (`HYB-E3013`) for captured-write escapes; read-only captures are outside — Go's DRF-SC with `-race` as the backstop"*). When a closure passed to a foreign Go call (`<pkg>.<fn>` or foreign receiver method) reads a captured `may_share` binding `x`, rule `O5.3` places an **Escape-Freeze witness** on `share_group(x)`. Any post-escape mutation in the enclosing `.hy` function, alias peer, or enclosing loop backedge is rejected at check time (`HYB-E3014`), while pre-populated read-only tables (`http.HandleFunc` routing/config maps) remain accepted with zero `managed` boilerplate (`12/12` conformance cases passing: `R1`–`R6` reject, `A1`–`A6` accept).
