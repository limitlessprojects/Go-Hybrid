"""Fair roster draws from a policy with hidden choices (issue #4896, spike).

Modules
-------
policy      nondeterministic step function + validity, including the
            ``split_select`` refactor used by the contract test
determinize subset construction, big-integer suffix counting, rank codec,
            canonical witness, plus the naive :class:`PathCounter` control
oracle      independent whole-roster enumerator (no automaton involved)
samplers    the three implementations that get compared
stats       chi-square / Wilson / risk ratio / entropy
"""

from .determinize import PathCounter, SetAutomaton
from .policy import POLICY_A, POLICY_A_SPLIT, POLICY_B, Policy, scale_policy

__all__ = [
    "PathCounter",
    "SetAutomaton",
    "POLICY_A",
    "POLICY_A_SPLIT",
    "POLICY_B",
    "Policy",
    "scale_policy",
]
