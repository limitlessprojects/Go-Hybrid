//! Finite-language distinguishers for the `hyb distinguish` command (#4807).
//!
//! This crate deliberately supports a small, checkable recognizer subset. It
//! extracts that subset from the real `hy-syntax` AST, exhaustively explores
//! the reachable product automaton, and then solves the complete pairwise
//! distinguishing-cover problem. Unsupported statements are rejected rather
//! than sampled or approximated.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use hy_syntax::{
    AssignOp, BinOp, Block, Decl, Expr, FnDecl, ForKind, LitKind, LocalDecl, LocalKind, Stmt,
    TypeExpr, UnOp,
};

/// One deterministic, total byte recognizer.
#[derive(Debug, Clone)]
pub struct Recognizer {
    pub name: String,
    /// `transitions[state][byte]`.
    pub transitions: Vec<[usize; 256]>,
    /// Acceptance at end of input for each state.
    pub accepting: Vec<bool>,
}

#[derive(Debug, Clone, Copy)]
pub struct ExtractLimits {
    pub max_states: usize,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self { max_states: 10_000 }
    }
}

#[derive(Debug, Clone)]
pub struct ExtractError {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ExtractError {}

#[derive(Debug, Clone, Copy)]
pub struct AnalysisLimits {
    pub max_product_states: usize,
    pub max_cover_nodes: usize,
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            max_product_states: 250_000,
            max_cover_nodes: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Certification {
    /// Product exploration and the exact cover search both completed.
    Minimal,
    /// The probe set is a verified cover, but the cover search stopped at a
    /// budget before proving no smaller set exists.
    UpperBound,
    /// Product exploration stopped at a budget. Even pairwise equivalence is
    /// unproven, so callers must not treat the observed classes as exact.
    Inconclusive,
}

impl Certification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::UpperBound => "upper_bound",
            Self::Inconclusive => "inconclusive",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Probe {
    /// The shortest lexicographically smallest byte string reaching one
    /// distinct answer vector.
    pub input: Vec<u8>,
    pub answers: Vec<bool>,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub candidate_names: Vec<String>,
    pub probes: Vec<Probe>,
    /// Candidate indexes with identical behavior in every explored state.
    pub equivalence_classes: Vec<Vec<usize>>,
    pub certificate: Certificate,
    pub per_candidate_states: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Certificate {
    pub status: Certification,
    pub product_states: usize,
    pub distinct_answer_vectors: usize,
    pub distinguishable_pairs: usize,
    pub product_exhaustive: bool,
    pub cover_exhaustive: bool,
    pub cover_nodes: usize,
    pub probe_count: usize,
    pub total_probe_bytes: usize,
    pub max_probe_bytes: usize,
}

impl Recognizer {
    pub fn new(
        name: impl Into<String>,
        transitions: Vec<[usize; 256]>,
        accepting: Vec<bool>,
    ) -> Result<Self, String> {
        if transitions.is_empty() {
            return Err("recognizer has no states".into());
        }
        if transitions.len() != accepting.len() {
            return Err("transition and acceptance state counts differ".into());
        }
        for (state, row) in transitions.iter().enumerate() {
            if let Some(&target) = row.iter().find(|&&target| target >= transitions.len()) {
                return Err(format!(
                    "state {state} has an out-of-range transition to {target}"
                ));
            }
        }
        Ok(Self {
            name: name.into(),
            transitions,
            accepting,
        })
    }

    pub fn accepts(&self, input: &[u8]) -> bool {
        let mut state = 0;
        for &byte in input {
            state = self.transitions[state][usize::from(byte)];
        }
        self.accepting[state]
    }
}

/// Analyze all candidates together.
///
/// Exhausting the product automaton proves language equivalence and every
/// distinguishable pair. The exact set-cover search then selects the smallest
/// probe set that separates all of them. A state budget stops product search;
/// a node budget stops cover search. Neither condition is reported as
/// `minimal`.
pub fn analyze(recognizers: &[Recognizer], limits: AnalysisLimits) -> Analysis {
    assert!(
        !recognizers.is_empty(),
        "at least one recognizer is required"
    );
    let start = vec![0usize; recognizers.len()];
    let mut queue = VecDeque::from([start.clone()]);
    let mut seen: HashSet<Vec<usize>> = HashSet::from([start.clone()]);
    let mut product_exhaustive = true;
    let mut product_states = 0usize;

    while let Some(tuple) = queue.pop_front() {
        product_states += 1;
        for byte in 0u16..=255 {
            let next: Vec<usize> = tuple
                .iter()
                .zip(recognizers)
                .map(|(&state, recognizer)| recognizer.transitions[state][byte as usize])
                .collect();
            if seen.contains(&next) {
                continue;
            }
            if product_states + queue.len() >= limits.max_product_states {
                product_exhaustive = false;
                break;
            }
            seen.insert(next.clone());
            queue.push_back(next);
        }
        if !product_exhaustive {
            break;
        }
    }

    // A second BFS over exactly the explored product records the
    // lexicographically first shortest word for each reachable tuple. Bytes
    // are visited in ascending order, so the first word found for a tuple is
    // the deterministic representative.
    let mut tuple_words: HashMap<Vec<usize>, Vec<u8>> = HashMap::new();
    tuple_words.insert(start.clone(), Vec::new());
    let mut word_queue = VecDeque::from([start]);
    while let Some(tuple) = word_queue.pop_front() {
        let word = tuple_words[&tuple].clone();
        for byte in 0u16..=255 {
            let next: Vec<usize> = tuple
                .iter()
                .zip(recognizers)
                .map(|(&state, recognizer)| recognizer.transitions[state][byte as usize])
                .collect();
            if !seen.contains(&next) {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(entry) =
                tuple_words.entry(next.clone())
            {
                let mut candidate = word.clone();
                candidate.push(byte as u8);
                entry.insert(candidate);
                word_queue.push_back(next);
            }
        }
    }

    let mut vectors: BTreeMap<Vec<bool>, Vec<u8>> = BTreeMap::new();
    for tuple in &seen {
        let answers: Vec<bool> = tuple
            .iter()
            .zip(recognizers)
            .map(|(&state, recognizer)| recognizer.accepting[state])
            .collect();
        let Some(word) = tuple_words.get(tuple) else {
            continue;
        };
        match vectors.entry(answers) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(word.clone());
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if word_order(word, entry.get()).is_lt() {
                    entry.insert(word.clone());
                }
            }
        }
    }

    let distinct: Vec<(Vec<bool>, Vec<u8>)> = vectors.into_iter().collect();
    let distinct_answer_vectors = distinct.len();
    let pair_count = recognizers.len() * recognizers.len().saturating_sub(1) / 2;
    let words = pair_words(pair_count);
    let mut distinguishable_pairs = 0usize;
    let mut probe_pair_masks = Vec::with_capacity(distinct.len());
    for _ in distinct.iter() {
        probe_pair_masks.push(vec![0u64; words]);
    }
    for left in 0..recognizers.len() {
        for right in left + 1..recognizers.len() {
            let pair = pair_index(left, right, recognizers.len());
            let differing: Vec<usize> = distinct
                .iter()
                .enumerate()
                .filter_map(|(probe, (answers, _))| {
                    (answers[left] != answers[right]).then_some(probe)
                })
                .collect();
            if differing.is_empty() {
                continue;
            }
            distinguishable_pairs += 1;
            for probe in differing {
                set_bit(&mut probe_pair_masks[probe], pair);
            }
        }
    }

    let equivalence_classes = equivalence_classes(&distinct, recognizers.len());
    let usable: Vec<(Vec<u8>, Vec<bool>, Vec<u64>)> = distinct
        .into_iter()
        .zip(probe_pair_masks)
        .filter(|(_, mask)| mask.iter().any(|&word| word != 0))
        .map(|((answers, word), mask)| (word, answers, mask))
        .collect();
    let (selected, cover_exhaustive, cover_nodes) =
        exact_cover(&usable, words, limits.max_cover_nodes);
    let probes: Vec<Probe> = selected
        .into_iter()
        .map(|index| Probe {
            input: usable[index].0.clone(),
            answers: usable[index].1.clone(),
        })
        .collect();

    let total_probe_bytes = probes.iter().map(|probe| probe.input.len()).sum();
    let max_probe_bytes = probes
        .iter()
        .map(|probe| probe.input.len())
        .max()
        .unwrap_or(0);
    let probe_count = probes.len();
    let status = if !product_exhaustive {
        Certification::Inconclusive
    } else if cover_exhaustive {
        Certification::Minimal
    } else {
        Certification::UpperBound
    };

    Analysis {
        candidate_names: recognizers.iter().map(|r| r.name.clone()).collect(),
        probes,
        equivalence_classes,
        certificate: Certificate {
            status,
            product_states,
            distinct_answer_vectors,
            distinguishable_pairs,
            product_exhaustive,
            cover_exhaustive,
            cover_nodes,
            probe_count,
            total_probe_bytes,
            max_probe_bytes,
        },
        per_candidate_states: recognizers.iter().map(|r| r.transitions.len()).collect(),
    }
}

fn word_order(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn pair_index(left: usize, right: usize, candidates: usize) -> usize {
    let (left, right) = if left < right {
        (left, right)
    } else {
        (right, left)
    };
    left * (2 * candidates - left - 1) / 2 + (right - left - 1)
}

fn pair_words(pairs: usize) -> usize {
    pairs.div_ceil(64)
}

fn set_bit(words: &mut [u64], bit: usize) {
    words[bit / 64] |= 1u64 << (bit % 64);
}

fn bit_set(words: &[u64], bit: usize) -> bool {
    words[bit / 64] & (1u64 << (bit % 64)) != 0
}

fn mask_subset(left: &[u64], right: &[u64]) -> bool {
    left.iter().zip(right).all(|(&a, &b)| a & !b == 0)
}

fn mask_or_into(dst: &mut [u64], src: &[u64]) {
    for (a, b) in dst.iter_mut().zip(src) {
        *a |= *b;
    }
}

fn new_words(dst: &mut [u64], src: &[u64]) {
    dst.copy_from_slice(src);
}

fn count_bits(words: &[u64]) -> usize {
    words.iter().map(|word| word.count_ones() as usize).sum()
}

fn better_solution(
    left: &[usize],
    right: &[usize],
    probes: &[(Vec<u8>, Vec<bool>, Vec<u64>)],
) -> bool {
    if left.len() != right.len() {
        return left.len() < right.len();
    }
    let left_bytes: usize = left.iter().map(|&i| probes[i].0.len()).sum();
    let right_bytes: usize = right.iter().map(|&i| probes[i].0.len()).sum();
    if left_bytes != right_bytes {
        return left_bytes < right_bytes;
    }
    let left_max = left.iter().map(|&i| probes[i].0.len()).max().unwrap_or(0);
    let right_max = right.iter().map(|&i| probes[i].0.len()).max().unwrap_or(0);
    if left_max != right_max {
        return left_max < right_max;
    }
    left.iter()
        .map(|&i| &probes[i].0)
        .cmp(right.iter().map(|&i| &probes[i].0))
        .is_lt()
}

fn exact_cover(
    probes: &[(Vec<u8>, Vec<bool>, Vec<u64>)],
    words: usize,
    max_nodes: usize,
) -> (Vec<usize>, bool, usize) {
    if probes.is_empty() || words == 0 {
        return (Vec::new(), true, 0);
    }
    let mut universe = vec![0u64; words];
    for (_, _, mask) in probes {
        mask_or_into(&mut universe, mask);
    }

    let mut greedy_covered = vec![0u64; words];
    let mut greedy = Vec::new();
    while count_bits(&greedy_covered) < count_bits(&universe) {
        let best = probes
            .iter()
            .enumerate()
            .map(|(index, (input, _, mask))| {
                let mut newly = vec![0u64; words];
                for i in 0..words {
                    newly[i] = mask[i] & !greedy_covered[i];
                }
                (index, count_bits(&newly), input.len())
            })
            .filter(|(_, covered, _)| *covered > 0)
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));
        let Some((index, _, _)) = best else {
            break;
        };
        greedy.push(index);
        mask_or_into(&mut greedy_covered, &probes[index].2);
    }
    let mut search = CoverSearch {
        probes,
        universe: &universe,
        max_nodes,
        best: greedy,
        nodes: 0,
        exhaustive: true,
    };
    search.visit(&mut vec![0u64; words], &mut Vec::new());
    (search.best, search.exhaustive, search.nodes)
}

struct CoverSearch<'a> {
    probes: &'a [(Vec<u8>, Vec<bool>, Vec<u64>)],
    universe: &'a [u64],
    max_nodes: usize,
    best: Vec<usize>,
    nodes: usize,
    exhaustive: bool,
}

impl CoverSearch<'_> {
    fn visit(&mut self, covered: &mut [u64], chosen: &mut Vec<usize>) {
        self.nodes += 1;
        if self.nodes > self.max_nodes {
            self.exhaustive = false;
            return;
        }
        if mask_subset(self.universe, covered) {
            if better_solution(chosen, &self.best, self.probes) {
                self.best.clone_from(chosen);
            }
            return;
        }
        // Any completion from here adds at least one probe. If the incumbent
        // already has that many probes, this branch cannot improve the primary
        // objective (the complete case above handles an equal-size solution).
        if chosen.len() >= self.best.len() {
            return;
        }
        let mut candidates: Option<Vec<usize>> = None;
        for bit in 0..self.universe.len() * 64 {
            if !bit_set(self.universe, bit) || bit_set(covered, bit) {
                continue;
            }
            let available: Vec<usize> = self
                .probes
                .iter()
                .enumerate()
                .filter_map(|(index, (_, _, mask))| bit_set(mask, bit).then_some(index))
                .collect();
            if candidates
                .as_ref()
                .is_none_or(|old| available.len() < old.len())
            {
                candidates = Some(available);
            }
        }
        let Some(mut candidates) = candidates else {
            return;
        };
        candidates.sort_by(|&a, &b| {
            let gain = |index: usize| {
                self.probes[index]
                    .2
                    .iter()
                    .zip(covered.iter())
                    .map(|(&mask, &done)| (mask & !done).count_ones())
                    .sum::<u32>()
            };
            gain(b)
                .cmp(&gain(a))
                .then_with(|| self.probes[a].0.len().cmp(&self.probes[b].0.len()))
                .then_with(|| self.probes[a].0.cmp(&self.probes[b].0))
        });
        for index in candidates {
            let old = covered.to_vec();
            mask_or_into(covered, &self.probes[index].2);
            chosen.push(index);
            self.visit(covered, chosen);
            chosen.pop();
            new_words(covered, &old);
            if !self.exhaustive {
                return;
            }
        }
    }
}

fn equivalence_classes(vectors: &[(Vec<bool>, Vec<u8>)], candidates: usize) -> Vec<Vec<usize>> {
    let mut classes: Vec<Vec<usize>> = Vec::new();
    for left in 0..candidates {
        if let Some(class) = classes.iter_mut().find(|class| {
            let anchor = class[0];
            vectors
                .iter()
                .all(|(answers, _)| answers[anchor] == answers[left])
        }) {
            class.push(left);
        } else {
            classes.push(vec![left]);
        }
    }
    classes
}

// ---------------------------------------------------------------------------
// Extraction from the deliberately bounded `.hy` recognizer subset
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Int(i64),
    Bool(bool),
}

type Env = HashMap<String, Value>;

/// Extract a recognizer from source accepted by `hy-syntax`.
///
/// Supported shape:
/// * one function named `function_name`, with one `[]byte`/`[]u8` input and a
///   `bool` result;
/// * scalar `var`/`:=` initializers before one `for b in input` loop;
/// * a finite transition body built from integer arithmetic, assignments,
///   `if`/`else`, and scalar local declarations;
/// * one Boolean return expression after the loop.
///
/// The input byte is bound to `b` (or to the second variable of
/// `for _, b in input`). Unsupported AST nodes, non-total/rejected statements,
/// and state growth past `limits.max_states` are errors.
pub fn extract_recognizer(
    name: impl Into<String>,
    path: &str,
    source: &str,
    function_name: &str,
    limits: ExtractLimits,
) -> Result<Recognizer, ExtractError> {
    let name = name.into();
    let file = hy_syntax::parse_file(path, source).map_err(|diagnostics| ExtractError {
        path: path.to_string(),
        message: diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render_head())
            .collect::<Vec<_>>()
            .join("; "),
    })?;
    let function = file
        .decls
        .iter()
        .find_map(|declaration| match declaration {
            Decl::Fn(function) if function.name == function_name => Some(function),
            _ => None,
        })
        .ok_or_else(|| ExtractError {
            path: path.to_string(),
            message: format!("function `{function_name}` not found"),
        })?;
    validate_signature(function, path)?;
    let input_name = function.params[0].name.clone();
    let body = function.body.as_ref().ok_or_else(|| ExtractError {
        path: path.to_string(),
        message: "recognizer function has no body".into(),
    })?;
    let (initial, loop_block, loop_var, return_expr) =
        split_recognizer_body(body, &input_name, path)?;
    let persistent: BTreeSet<String> = initial.keys().cloned().collect();
    let mut states = vec![initial];
    let mut state_index: HashMap<Vec<(String, i64)>, usize> = HashMap::new();
    state_index.insert(env_key(&states[0]), 0);
    let mut transitions: Vec<[usize; 256]> = Vec::new();
    let mut accepting = Vec::new();

    let mut cursor = 0usize;
    while cursor < states.len() {
        let state = states[cursor].clone();
        accepting.push(
            eval_bool(return_expr, &state).map_err(|message| ExtractError {
                path: path.to_string(),
                message: format!("invalid EOF acceptance expression: {message}"),
            })?,
        );
        let mut row = [0usize; 256];
        for byte in 0u16..=255 {
            let mut transition_env = state.clone();
            transition_env.insert(loop_var.clone(), Value::Int(i64::from(byte as u8)));
            execute_block(loop_block, &mut transition_env, path)?;
            transition_env.retain(|key, _| persistent.contains(key));
            let key = env_key(&transition_env);
            let next = if let Some(&index) = state_index.get(&key) {
                index
            } else {
                if states.len() >= limits.max_states {
                    return Err(ExtractError {
                        path: path.to_string(),
                        message: format!(
                            "finite-state extraction exceeded {} states",
                            limits.max_states
                        ),
                    });
                }
                let index = states.len();
                states.push(transition_env);
                state_index.insert(key, index);
                index
            };
            row[usize::from(byte as u8)] = next;
        }
        transitions.push(row);
        cursor += 1;
    }
    Recognizer::new(name, transitions, accepting).map_err(|message| ExtractError {
        path: path.to_string(),
        message,
    })
}

fn validate_signature(function: &FnDecl, path: &str) -> Result<(), ExtractError> {
    if function.receiver.is_some() || !function.type_params.is_empty() {
        return Err(ExtractError {
            path: path.to_string(),
            message: "recognizer must be a non-generic free function".into(),
        });
    }
    if function.params.len() != 1 {
        return Err(ExtractError {
            path: path.to_string(),
            message: "recognizer must have exactly one input parameter".into(),
        });
    }
    let input_ty = &function.params[0].ty;
    let is_byte_slice = matches!(
        input_ty,
        TypeExpr::Slice { elem, .. }
            if matches!(&**elem, TypeExpr::Named { name, .. } if name == "byte" || name == "u8")
    );
    if !is_byte_slice {
        return Err(ExtractError {
            path: path.to_string(),
            message: "recognizer input must be `[]byte` or `[]u8`".into(),
        });
    }
    if function.results.len() != 1
        || !matches!(&function.results[0].ty, TypeExpr::Named { name, .. } if name == "bool")
    {
        return Err(ExtractError {
            path: path.to_string(),
            message: "recognizer must return exactly one `bool`".into(),
        });
    }
    Ok(())
}

fn split_recognizer_body<'a>(
    body: &'a Block,
    input_name: &str,
    path: &str,
) -> Result<(Env, &'a Block, String, &'a Expr), ExtractError> {
    let mut initial = Env::new();
    let mut loop_block = None;
    let mut loop_var = None;
    let mut return_expr = None;
    for statement in &body.stmts {
        match statement {
            Stmt::Local(local) if loop_block.is_none() => {
                declare_initial(local, &mut initial, path)?;
            }
            Stmt::For {
                kind: ForKind::Iterate { vars, expr },
                body: iteration,
                ..
            } if loop_block.is_none() => {
                if !matches!(expr, Expr::Ident { name, .. } if name == input_name) {
                    return Err(ExtractError {
                        path: path.to_string(),
                        message: format!("recognizer loop must iterate over `{input_name}`"),
                    });
                }
                let variable = match vars.as_slice() {
                    [byte] => byte.clone(),
                    [_, byte] => byte.clone(),
                    _ => {
                        return Err(ExtractError {
                            path: path.to_string(),
                            message: "recognizer loop must bind zero or one index and one byte"
                                .into(),
                        });
                    }
                };
                loop_block = Some(iteration);
                loop_var = Some(variable);
            }
            Stmt::Return { values, .. } if loop_block.is_some() && return_expr.is_none() => {
                if values.len() != 1 {
                    return Err(ExtractError {
                        path: path.to_string(),
                        message: "recognizer return must contain one Boolean expression".into(),
                    });
                }
                return_expr = Some(&values[0]);
            }
            _ => {
                return Err(ExtractError {
                    path: path.to_string(),
                    message: "recognizer body must be scalar initializers, one byte loop, then one return".into(),
                });
            }
        }
    }
    let loop_block = loop_block.ok_or_else(|| ExtractError {
        path: path.to_string(),
        message: "recognizer body has no `for b in input` loop".into(),
    })?;
    let loop_var = loop_var.expect("loop block and variable are set together");
    let return_expr = return_expr.ok_or_else(|| ExtractError {
        path: path.to_string(),
        message: "recognizer body has no return after the loop".into(),
    })?;
    Ok((initial, loop_block, loop_var, return_expr))
}

fn declare_initial(local: &LocalDecl, env: &mut Env, path: &str) -> Result<(), ExtractError> {
    if local.names.len() != 1 || local.values.len() > 1 {
        return Err(ExtractError {
            path: path.to_string(),
            message: "pre-loop recognizer declarations must be scalar with at most one initializer"
                .into(),
        });
    }
    if !matches!(local.kind, LocalKind::Var | LocalKind::Define) {
        return Err(ExtractError {
            path: path.to_string(),
            message: "pre-loop recognizer declarations must be `var` or `:=`".into(),
        });
    }
    let value = if let Some(initializer) = local.values.first() {
        eval(initializer, env).map_err(|message| ExtractError {
            path: path.to_string(),
            message: format!("invalid initializer for `{}`: {message}", local.names[0]),
        })?
    } else if local.kind == LocalKind::Var {
        Value::Int(0)
    } else {
        return Err(ExtractError {
            path: path.to_string(),
            message: "pre-loop recognizer declaration needs an initializer".into(),
        });
    };
    if !matches!(value, Value::Int(_)) {
        return Err(ExtractError {
            path: path.to_string(),
            message: "finite recognizer state must be integer".into(),
        });
    }
    env.insert(local.names[0].clone(), value);
    Ok(())
}

fn env_key(env: &Env) -> Vec<(String, i64)> {
    env.iter()
        .filter_map(|(name, value)| match value {
            Value::Int(value) => Some((name.clone(), *value)),
            Value::Bool(_) => None,
        })
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect()
}

fn execute_block(block: &Block, env: &mut Env, path: &str) -> Result<(), ExtractError> {
    let mut declarations: Vec<(String, Option<Value>)> = Vec::new();
    for statement in &block.stmts {
        if let Stmt::Local(local) = statement {
            if local.names.len() != 1 || local.values.len() != 1 {
                return Err(ExtractError {
                    path: path.to_string(),
                    message: "transition locals must be scalar with one initializer".into(),
                });
            }
            let value = eval(&local.values[0], env).map_err(|message| ExtractError {
                path: path.to_string(),
                message: format!("invalid transition local: {message}"),
            })?;
            let name = local.names[0].clone();
            let previous = env.insert(name.clone(), value);
            declarations.push((name, previous));
        } else {
            execute_statement(statement, env, path)?;
        }
    }
    for (name, previous) in declarations.into_iter().rev() {
        if let Some(previous) = previous {
            env.insert(name, previous);
        } else {
            env.remove(&name);
        }
    }
    Ok(())
}

fn execute_statement(statement: &Stmt, env: &mut Env, path: &str) -> Result<(), ExtractError> {
    match statement {
        Stmt::Assign {
            targets,
            op,
            values,
            ..
        } => {
            if targets.len() != 1 || values.len() != 1 {
                return Err(ExtractError {
                    path: path.to_string(),
                    message: "transition assignments must be scalar".into(),
                });
            }
            let hy_syntax::PlaceKind::Ident(target) = &targets[0].kind else {
                return Err(ExtractError {
                    path: path.to_string(),
                    message: "transition assignments must target a scalar variable".into(),
                });
            };
            let current = env.get(target).cloned().ok_or_else(|| ExtractError {
                path: path.to_string(),
                message: format!("assignment to unknown variable `{target}`"),
            })?;
            let rhs = eval(&values[0], env).map_err(|message| ExtractError {
                path: path.to_string(),
                message: format!("invalid transition expression: {message}"),
            })?;
            let result = apply_assignment(current, *op, rhs).map_err(|message| ExtractError {
                path: path.to_string(),
                message: format!("invalid assignment to `{target}`: {message}"),
            })?;
            env.insert(target.clone(), result);
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if init.is_some() {
                return Err(ExtractError {
                    path: path.to_string(),
                    message: "if initializers are outside the recognizer subset".into(),
                });
            }
            let before = env.clone();
            if eval_bool(cond, env).map_err(|message| ExtractError {
                path: path.to_string(),
                message: format!("invalid transition condition: {message}"),
            })? {
                execute_block(then, env, path)?;
            } else if let Some(otherwise) = otherwise {
                execute_statement(otherwise, env, path)?;
            } else {
                *env = before;
            }
        }
        Stmt::Block { block, .. } => execute_block(block, env, path)?,
        _ => {
            return Err(ExtractError {
                path: path.to_string(),
                message: "unsupported control flow inside recognizer transition".into(),
            });
        }
    }
    Ok(())
}

fn apply_assignment(current: Value, op: AssignOp, rhs: Value) -> Result<Value, String> {
    match op {
        AssignOp::Assign => Ok(rhs),
        AssignOp::Inc => match current {
            Value::Int(value) => Ok(Value::Int(value.checked_add(1).ok_or("integer overflow")?)),
            _ => Err("increment requires integer".into()),
        },
        AssignOp::Dec => match current {
            Value::Int(value) => Ok(Value::Int(value.checked_sub(1).ok_or("integer overflow")?)),
            _ => Err("decrement requires integer".into()),
        },
        operator => {
            let (current, rhs) = match (current, rhs) {
                (Value::Int(current), Value::Int(rhs)) => (current, rhs),
                _ => return Err("compound assignment requires integers".into()),
            };
            let result = match operator {
                AssignOp::AddAssign => current.checked_add(rhs),
                AssignOp::SubAssign => current.checked_sub(rhs),
                AssignOp::MulAssign => current.checked_mul(rhs),
                AssignOp::DivAssign => current.checked_div(rhs),
                AssignOp::ModAssign => current.checked_rem(rhs),
                AssignOp::BitOrAssign => Some(current | rhs),
                AssignOp::BitAndAssign => Some(current & rhs),
                AssignOp::BitXorAssign => Some(current ^ rhs),
                AssignOp::ShlAssign => rhs
                    .try_into()
                    .ok()
                    .and_then(|shift: u32| current.checked_shl(shift)),
                AssignOp::ShrAssign => rhs
                    .try_into()
                    .ok()
                    .and_then(|shift: u32| current.checked_shr(shift)),
                AssignOp::Assign | AssignOp::Inc | AssignOp::Dec => unreachable!(),
            };
            result
                .map(Value::Int)
                .ok_or_else(|| "arithmetic overflow or division by zero".into())
        }
    }
}

fn eval_bool(expr: &Expr, env: &Env) -> Result<bool, String> {
    match eval(expr, env)? {
        Value::Bool(value) => Ok(value),
        Value::Int(_) => Err("expected Boolean expression".into()),
    }
}

fn eval(expr: &Expr, env: &Env) -> Result<Value, String> {
    match expr {
        Expr::Ident { name, .. } => env
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown variable `{name}`")),
        Expr::Lit { kind, .. } => match kind {
            LitKind::Int(text) => Ok(Value::Int(parse_int(text)?)),
            LitKind::Bool(value) => Ok(Value::Bool(*value)),
            _ => Err("only integer and Boolean literals are supported".into()),
        },
        Expr::Paren { inner, .. } => eval(inner, env),
        Expr::Unary { op, operand, .. } => match (op, eval(operand, env)?) {
            (UnOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
            (UnOp::Neg, Value::Int(value)) => value
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| "integer overflow".into()),
            _ => Err("unary operator has incompatible operand".into()),
        },
        Expr::Binary { op, lhs, rhs, .. } => {
            let lhs = eval(lhs, env)?;
            let rhs = eval(rhs, env)?;
            match (op, lhs, rhs) {
                (BinOp::And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
                (BinOp::Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),
                (BinOp::Eq, a, b) => Ok(Value::Bool(a == b)),
                (BinOp::Ne, a, b) => Ok(Value::Bool(a != b)),
                (BinOp::Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
                (BinOp::Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
                (BinOp::Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
                (BinOp::Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
                (op, Value::Int(a), Value::Int(b)) => {
                    let value = match op {
                        BinOp::Add => a.checked_add(b),
                        BinOp::Sub => a.checked_sub(b),
                        BinOp::Mul => a.checked_mul(b),
                        BinOp::Div => a.checked_div(b),
                        BinOp::Mod => a.checked_rem(b),
                        BinOp::BitOr => Some(a | b),
                        BinOp::BitXor => Some(a ^ b),
                        BinOp::BitAnd => Some(a & b),
                        BinOp::Shl => b
                            .try_into()
                            .ok()
                            .and_then(|shift: u32| a.checked_shl(shift)),
                        BinOp::Shr => b
                            .try_into()
                            .ok()
                            .and_then(|shift: u32| a.checked_shr(shift)),
                        _ => None,
                    };
                    value.map(Value::Int).ok_or_else(|| {
                        "integer overflow, invalid shift, or division by zero".into()
                    })
                }
                _ => Err("operator has incompatible operands".into()),
            }
        }
        _ => Err("unsupported expression in finite recognizer subset".into()),
    }
}

fn parse_int(text: &str) -> Result<i64, String> {
    let normalized = text.replace('_', "");
    let (radix, digits) = if let Some(rest) = normalized.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = normalized.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = normalized.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, normalized.as_str())
    };
    i64::from_str_radix(digits, radix).map_err(|_| format!("invalid integer literal `{text}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, byte: u8) -> Recognizer {
        let mut transitions = vec![[0usize; 256]; 2];
        transitions[0][usize::from(byte)] = 1;
        transitions[1][usize::from(byte)] = 1;
        Recognizer::new(name, transitions, vec![false, true]).unwrap()
    }

    fn baseline(name: &str) -> Recognizer {
        Recognizer::new(name, vec![[0usize; 256]], vec![false]).unwrap()
    }

    fn equivalent_extra_state_baseline(name: &str) -> Recognizer {
        let mut transitions = vec![[0usize; 256]; 2];
        for byte in 0u16..=255 {
            transitions[0][usize::from(byte as u8)] = 1;
            transitions[1][usize::from(byte as u8)] = 0;
        }
        Recognizer::new(name, transitions, vec![false, false]).unwrap()
    }

    #[test]
    fn four_tags_plus_equivalent_baseline_need_all_four_probes() {
        let candidates = vec![
            baseline("base"),
            tag("alpha", b'A'),
            tag("beta", b'B'),
            tag("gamma", b'C'),
            tag("delta", b'D'),
            equivalent_extra_state_baseline("base-extra-state"),
        ];
        let result = analyze(&candidates, AnalysisLimits::default());
        assert_eq!(result.certificate.status, Certification::Minimal);
        assert_eq!(result.certificate.probe_count, 4);
        assert_eq!(
            result.equivalence_classes,
            vec![vec![0, 5], vec![1], vec![2], vec![3], vec![4]]
        );
        let mut witnesses: Vec<Vec<u8>> = result.probes.iter().map(|p| p.input.clone()).collect();
        witnesses.sort();
        assert_eq!(
            witnesses,
            vec![b"A".to_vec(), b"B".to_vec(), b"C".to_vec(), b"D".to_vec()]
        );
    }

    fn flag_recognizer(code: u8) -> Recognizer {
        let mut transitions = vec![[0usize; 256]; 8];
        for (state, row) in transitions.iter_mut().enumerate() {
            for byte in 0u16..=255 {
                let bit = match byte as u8 {
                    b'+' => 4,
                    b'0' => 2,
                    b'_' => 1,
                    _ => 0,
                };
                row[byte as usize] = state | bit;
            }
        }
        let accepting = (0..8)
            .map(|seen| {
                (seen == 6 && code & 1 != 0)
                    || (seen == 2 && code & 2 != 0)
                    || (seen == 1 && code & 4 != 0)
            })
            .collect();
        Recognizer::new(format!("grammar-{code:03b}"), transitions, accepting).unwrap()
    }

    #[test]
    fn eight_flag_signatures_have_three_probe_minimum() {
        let candidates: Vec<Recognizer> = (0..8).map(flag_recognizer).collect();
        let result = analyze(&candidates, AnalysisLimits::default());
        assert_eq!(result.certificate.status, Certification::Minimal);
        assert_eq!(result.certificate.product_states, 8);
        assert_eq!(result.certificate.distinct_answer_vectors, 4);
        assert_eq!(result.certificate.distinguishable_pairs, 28);
        assert_eq!(result.probes.len(), 3);
        assert_eq!(result.certificate.total_probe_bytes, 4);
        let mut witnesses: Vec<Vec<u8>> = result.probes.iter().map(|p| p.input.clone()).collect();
        witnesses.sort();
        assert_eq!(
            witnesses,
            vec![b"+0".to_vec(), b"0".to_vec(), b"_".to_vec()]
        );
    }

    #[test]
    fn product_budget_is_inconclusive_and_never_called_minimal() {
        let candidates: Vec<Recognizer> = (0..8).map(flag_recognizer).collect();
        let result = analyze(
            &candidates,
            AnalysisLimits {
                max_product_states: 2,
                max_cover_nodes: 1_000,
            },
        );
        assert_eq!(result.certificate.status, Certification::Inconclusive);
        assert!(!result.certificate.product_exhaustive);
    }

    #[test]
    fn cover_budget_returns_verified_upper_bound() {
        let candidates: Vec<Recognizer> = (0..8).map(flag_recognizer).collect();
        let result = analyze(
            &candidates,
            AnalysisLimits {
                max_product_states: 1_000,
                max_cover_nodes: 1,
            },
        );
        assert_eq!(result.certificate.status, Certification::UpperBound);
        assert!(result.certificate.product_exhaustive);
        assert!(!result.certificate.cover_exhaustive);
    }

    #[test]
    fn extracts_ast_backed_recognizer_and_rejects_unbounded_state() {
        let source = r#"
pub fn recognize(input []byte) bool {
    var seen int = 0
    for b in input {
        if b == 65 {
            seen = 1
        }
    }
    return seen == 1
}
"#;
        let recognizer = extract_recognizer(
            "alpha",
            "inline.hy",
            source,
            "recognize",
            ExtractLimits::default(),
        )
        .expect("supported recognizer must extract");
        assert_eq!(recognizer.transitions.len(), 2);
        assert!(recognizer.accepts(b"A"));
        assert!(!recognizer.accepts(b"B"));

        let unbounded = r#"
pub fn recognize(input []byte) bool {
    var n int = 0
    for b in input {
        n += 1
    }
    return n == 3
}
"#;
        let error = extract_recognizer(
            "unbounded",
            "unbounded.hy",
            unbounded,
            "recognize",
            ExtractLimits { max_states: 4 },
        )
        .expect_err("state budget must fail closed");
        assert!(error.message.contains("exceeded 4 states"));
    }

    #[test]
    fn unsupported_control_flow_fails_closed() {
        let source = r#"
pub fn recognize(input []byte) bool {
    var state int = 0
    for b in input {
        task worker()
    }
    return state == 0
}
"#;
        let error = extract_recognizer(
            "bad",
            "bad.hy",
            source,
            "recognize",
            ExtractLimits::default(),
        )
        .expect_err("task statements are outside the recognizer subset");
        assert!(error.message.contains("unsupported control flow"));
    }

    #[test]
    fn block_local_shadowing_does_not_mutate_outer_state() {
        let source = r#"
pub fn recognize(input []byte) bool {
    var state int
    for b in input {
        if state == 0 {
            state = 1
        }
        if state == 1 {
            state := 2
            state = 3
        }
    }
    return state == 1
}
"#;
        let recognizer = extract_recognizer(
            "shadowed",
            "shadowed.hy",
            source,
            "recognize",
            ExtractLimits::default(),
        )
        .expect("lexically scoped transition must extract");
        assert_eq!(recognizer.transitions.len(), 2);
        assert!(recognizer.accepts(b"A"));
        assert!(recognizer.accepts(b"AB"));
    }
}
