```hy
# Roster policy: 8 shifts assigned to volunteers A, B, C
# Constraints:
#   - Nobody serves twice in a row
#   - C can take at most two shifts
#   - Exactly one A-to-B handoff must be designated for training
# State: (previous_volunteer, c_shifts, handoff_selected)
# Visible symbol: next_volunteer
# Private choice: whether A->B handoff is selected (only on A->B transition)

type Volunteer = "A" | "B" | "C"
type State = (previous: Volunteer, c_shifts: 0..2, handoff_selected: bool)

init_state = (previous="", c_shifts=0, handoff_selected=false)

# Transition: consume one visible symbol (next_volunteer)
# Returns: new_state or none if constraint violated
def advance(state: State, next_volunteer: Volunteer) -> Option[State]:
    previous, c_shifts, handoff_selected = state
    
    # Nobody serves twice in a row
    if next_volunteer == previous:
        return none
    
    # C can take at most two shifts
    if next_volunteer == "C":
        if c_shifts >= 2:
            return none
        new_c_shifts = c_shifts + 1
    else:
        new_c_shifts = 0  # Reset C shift count when non-C volunteers
    
    # Exactly one A-to-B handoff must be designated for training
    new_handoff_selected = handoff_selected
    if next_volunteer == "B" and previous == "A":
        # A->B transition: can select or not select handoff
        # But we can only select it once (first A->B encounter)
        if not handoff_selected:
            new_handoff_selected = true
        # If already selected, stays true (we've found our one A->B handoff)
    
    return some (previous=next_volunteer, c_shifts=new_c_shifts, handoff_selected=new_handoff_selected)

# Policy: draw 8 shifts uniformly from valid roster-and-handoff pairs
# Visible output: the roster (sequence of 8 volunteers)
# Private witness: which A->B transition selected the handoff
# The draw must give exactly one ticket per publishable roster regardless of handoff choices

def policy_draw(horizon: 8) -> (roster: [Volunteer], error):
    # Start from init_state
    # Enumerate all valid paths of length horizon
    # Use subset determinization and rank-decoding for uniform draw
    pass
```
