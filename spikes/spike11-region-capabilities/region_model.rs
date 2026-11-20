use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REGION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Region(u64);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Value {
    regions: BTreeSet<Region>,
    known: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct State {
    held: BTreeSet<Region>,
}

#[derive(Clone, Debug, Default)]
struct Aggregate {
    value: Value,
    fields: BTreeMap<&'static str, Value>,
}

#[derive(Debug, Eq, PartialEq)]
enum Error {
    UnknownProvenance,
    MissingCapability(Region),
}

impl State {
    fn allocate(&mut self) -> Value {
        let region = Region(NEXT_REGION.fetch_add(1, Ordering::Relaxed));
        self.held.insert(region);
        Value { regions: [region].into_iter().collect(), known: true }
    }

    fn aggregate(&self, fields: impl IntoIterator<Item = (&'static str, Value)>) -> Aggregate {
        let fields: BTreeMap<_, _> = fields.into_iter().collect();
        let regions = fields.values().flat_map(|v| v.regions.iter().copied()).collect();
        let known = fields.values().all(|v| v.known);
        Aggregate { value: Value { regions, known }, fields }
    }

    fn project(&self, aggregate: &Aggregate, field: &'static str) -> Option<Value> {
        aggregate.fields.get(field).cloned()
    }

    fn access(&self, value: &Value) -> Result<(), Error> {
        if !value.known { return Err(Error::UnknownProvenance); }
        for region in &value.regions {
            if !self.held.contains(region) { return Err(Error::MissingCapability(*region)); }
        }
        Ok(())
    }

    fn transfer(&mut self, value: &Value, receiver: &mut State) -> Result<(), Error> {
        self.access(value)?;
        for region in &value.regions {
            self.held.remove(region);
            receiver.held.insert(*region);
        }
        Ok(())
    }

    // A join is safe only for capabilities held on every incoming path.
    fn join(left: &State, right: &State) -> State {
        State { held: left.held.intersection(&right.held).copied().collect() }
    }
}

// A function summary must say whether a returned reference is fresh or aliases
// one or more inputs. An absent summary fails closed.
fn summarized_return(inputs: &[Value], aliases: Option<&[usize]>, fresh: Option<Value>) -> Value {
    if let Some(indices) = aliases {
        let regions = indices.iter().flat_map(|i| inputs.get(*i).into_iter().flat_map(|v| v.regions.iter().copied())).collect();
        let known = indices.iter().all(|i| inputs.get(*i).map(|v| v.known).unwrap_or(false));
        Value { regions, known }
    } else {
        fresh.unwrap_or(Value { regions: BTreeSet::new(), known: false })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_cannot_be_used_after_its_region_capability_moves() {
        let mut sender = State::default();
        let mut receiver = State::default();
        let value = sender.allocate();
        let region = *value.regions.iter().next().unwrap();
        let alias = value.clone();
        sender.transfer(&value, &mut receiver).unwrap();
        assert_eq!(sender.access(&alias), Err(Error::MissingCapability(region)));
        assert_eq!(receiver.access(&value), Ok(()));
    }

    #[test]
    fn aggregate_transfer_moves_nested_regions_and_projection_checks_them() {
        let mut sender = State::default();
        let mut receiver = State::default();
        let child = sender.allocate();
        let region = *child.regions.iter().next().unwrap();
        let aggregate = sender.aggregate([("child", child.clone())]);
        let projection = sender.project(&aggregate, "child").unwrap();
        sender.transfer(&aggregate.value, &mut receiver).unwrap();
        assert_eq!(sender.access(&projection), Err(Error::MissingCapability(region)));
        assert_eq!(receiver.access(&projection), Ok(()));
    }

    #[test]
    fn control_flow_join_drops_capabilities_not_held_on_every_path() {
        let mut before = State::default();
        let value = before.allocate();
        let region = *value.regions.iter().next().unwrap();
        let mut transferred = before.clone();
        let mut receiver = State::default();
        transferred.transfer(&value, &mut receiver).unwrap();
        let joined = State::join(&before, &transferred);
        assert_eq!(joined.access(&value), Err(Error::MissingCapability(region)));
    }

    #[test]
    fn interprocedural_alias_summary_preserves_input_region() {
        let mut state = State::default();
        let input = state.allocate();
        let region = *input.regions.iter().next().unwrap();
        let returned = summarized_return(&[input.clone()], Some(&[0]), None);
        let mut receiver = State::default();
        state.transfer(&input, &mut receiver).unwrap();
        assert_eq!(state.access(&returned), Err(Error::MissingCapability(region)));
    }

    #[test]
    fn missing_external_call_summary_is_not_treated_as_fresh_or_safe() {
        let returned = summarized_return(&[], None, None);
        assert_eq!(State::default().access(&returned), Err(Error::UnknownProvenance));
    }

    #[test]
    fn allocations_in_separate_states_get_distinct_regions() {
        let first = State::default().allocate();
        let second = State::default().allocate();
        assert_ne!(first.regions, second.regions);
    }
}
