//! Parallel backward evaluation of the six placement-only opening plies.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::graph::{for_each_opening_successor, OpeningChild};
use crate::ranking::{
    opening_ply_range, rank_opening, rank_post_opening, unrank_opening, OpeningId,
    LOCKED_OPENING_DOMAIN,
};
use crate::retrograde::{Solution, Value};
use crate::Rules;

const UNKNOWN: u8 = 0;
const WIN: u8 = 1;
const LOSS: u8 = 2;
const DRAW: u8 = 3;

pub trait PostValues: Sync {
    fn value(&self, id: crate::ranking::PostOpeningId) -> Value;
}

impl PostValues for Solution {
    fn value(&self, id: crate::ranking::PostOpeningId) -> Value {
        self.value(id.get())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpeningLayerStats {
    pub ply: u8,
    pub states: u64,
    pub edges: u64,
    pub wins: u64,
    pub losses: u64,
    pub draws: u64,
}

pub struct OpeningSolution {
    values: Vec<u8>,
    layers: [OpeningLayerStats; 6],
}

impl OpeningSolution {
    pub fn value(&self, id: OpeningId) -> Value {
        decode(self.values[id.get() as usize])
    }

    pub fn initial_value(&self) -> Value {
        self.value(OpeningId::new(0).expect("initial opening ID exists"))
    }

    pub const fn layers(&self) -> &[OpeningLayerStats; 6] {
        &self.layers
    }
}

pub fn solve_opening(
    post_values: &impl PostValues,
    rules: Rules,
    threads: usize,
) -> Result<OpeningSolution, OpeningError> {
    validate_threads(threads)?;
    let values = vec![UNKNOWN; LOCKED_OPENING_DOMAIN as usize];
    let atomic_values = as_atomic_bytes(&values);
    let mut layers = [OpeningLayerStats::default(); 6];

    for ply in (0..=5_u8).rev() {
        let range = opening_ply_range(ply).expect("opening ply is in range");
        let length = (range.end - range.start) as usize;
        let chunk_size = length.div_ceil(threads).max(1);
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for first in (range.start..range.end).step_by(chunk_size) {
                let end = range.end.min(first.saturating_add(chunk_size as u32));
                handles.push(scope.spawn(move || {
                    solve_layer_chunk(first, end, atomic_values, post_values, rules)
                }));
            }
            join_results(handles)
        })?;

        let mut stats = OpeningLayerStats {
            ply,
            ..OpeningLayerStats::default()
        };
        for result in results {
            stats.states += result.states;
            stats.edges += result.edges;
            stats.wins += result.wins;
            stats.losses += result.losses;
            stats.draws += result.draws;
        }
        layers[ply as usize] = stats;
    }

    Ok(OpeningSolution { values, layers })
}

pub fn audit_opening(
    solution: &OpeningSolution,
    post_values: &impl PostValues,
    rules: Rules,
    threads: usize,
) -> Result<[OpeningLayerStats; 6], OpeningError> {
    validate_threads(threads)?;
    let mut layers = [OpeningLayerStats::default(); 6];
    for ply in 0..=5_u8 {
        let range = opening_ply_range(ply).expect("opening ply is in range");
        let length = (range.end - range.start) as usize;
        let chunk_size = length.div_ceil(threads).max(1);
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for first in (range.start..range.end).step_by(chunk_size) {
                let end = range.end.min(first.saturating_add(chunk_size as u32));
                handles
                    .push(scope.spawn(move || {
                        audit_layer_chunk(first, end, solution, post_values, rules)
                    }));
            }
            join_results(handles)
        })?;
        let mut stats = OpeningLayerStats {
            ply,
            ..OpeningLayerStats::default()
        };
        for result in results {
            stats.states += result.states;
            stats.edges += result.edges;
            stats.wins += result.wins;
            stats.losses += result.losses;
            stats.draws += result.draws;
        }
        layers[ply as usize] = stats;
    }
    Ok(layers)
}

fn solve_layer_chunk(
    first: u32,
    end: u32,
    opening_values: &[AtomicU8],
    post_values: &impl PostValues,
    rules: Rules,
) -> Result<OpeningLayerStats, OpeningError> {
    let mut stats = OpeningLayerStats::default();
    for raw in first..end {
        let (value, edges) = evaluate(raw, opening_values, post_values, rules)?;
        opening_values[raw as usize].store(encode(value), Ordering::Relaxed);
        accumulate(&mut stats, value, edges);
    }
    Ok(stats)
}

fn audit_layer_chunk(
    first: u32,
    end: u32,
    solution: &OpeningSolution,
    post_values: &impl PostValues,
    rules: Rules,
) -> Result<OpeningLayerStats, OpeningError> {
    let mut stats = OpeningLayerStats::default();
    for raw in first..end {
        let (expected, edges) = evaluate_reference(raw, solution, post_values, rules)?;
        let actual = decode(solution.values[raw as usize]);
        if actual != expected {
            return Err(OpeningError::AuditMismatch {
                id: raw,
                expected,
                actual,
            });
        }
        accumulate(&mut stats, actual, edges);
    }
    Ok(stats)
}

/// Independently replay one opening node through the readable rules engine.
///
/// The production solve uses the allocation-free generator in `graph`; this
/// audit intentionally uses `Position::legal_moves` plus checked `play`.
fn evaluate_reference(
    raw: u32,
    solution: &OpeningSolution,
    post_values: &impl PostValues,
    rules: Rules,
) -> Result<(Value, u64), OpeningError> {
    let id = OpeningId::new(raw).ok_or(OpeningError::OpeningIdOutOfRange(raw))?;
    let position = unrank_opening(id);
    let actions = position.legal_moves(rules);
    let edges = actions.len() as u64;
    let mut has_loss = false;
    let mut all_win = true;
    for action in actions {
        let child = position
            .play(action, rules)
            .expect("reference opening action remains legal");
        let value = if child.opening_complete() {
            post_values
                .value(rank_post_opening(&child).expect("reference unlocked child is rankable"))
        } else {
            solution.value(rank_opening(&child).expect("reference locked child is rankable"))
        };
        match value {
            Value::Loss => has_loss = true,
            Value::Win => {}
            Value::Draw => all_win = false,
        }
    }
    let value = if has_loss {
        Value::Win
    } else if all_win {
        Value::Loss
    } else {
        Value::Draw
    };
    Ok((value, edges))
}

fn evaluate(
    raw: u32,
    opening_values: &[AtomicU8],
    post_values: &impl PostValues,
    rules: Rules,
) -> Result<(Value, u64), OpeningError> {
    let id = OpeningId::new(raw).ok_or(OpeningError::OpeningIdOutOfRange(raw))?;
    let mut edges = 0_u64;
    let mut has_loss = false;
    let mut all_win = true;
    let mut error = None;
    for_each_opening_successor(id, rules, |child| {
        edges += 1;
        let value = match child {
            OpeningChild::Opening(child) => {
                let encoded = opening_values[child.get() as usize].load(Ordering::Relaxed);
                if encoded == UNKNOWN {
                    error = Some(OpeningError::UnresolvedChild {
                        parent: raw,
                        child: child.get(),
                    });
                    return;
                }
                decode(encoded)
            }
            OpeningChild::PostOpening(child) => post_values.value(child),
        };
        match value {
            Value::Loss => has_loss = true,
            Value::Win => {}
            Value::Draw => all_win = false,
        }
    });
    if let Some(error) = error {
        return Err(error);
    }
    let value = if has_loss {
        Value::Win
    } else if all_win {
        Value::Loss
    } else {
        Value::Draw
    };
    Ok((value, edges))
}

fn accumulate(stats: &mut OpeningLayerStats, value: Value, edges: u64) {
    stats.states += 1;
    stats.edges += edges;
    match value {
        Value::Win => stats.wins += 1,
        Value::Loss => stats.losses += 1,
        Value::Draw => stats.draws += 1,
    }
}

fn as_atomic_bytes(bytes: &[u8]) -> &[AtomicU8] {
    assert_eq!(std::mem::size_of::<AtomicU8>(), std::mem::size_of::<u8>());
    assert_eq!(std::mem::align_of::<AtomicU8>(), std::mem::align_of::<u8>());
    // SAFETY: every layer writes disjoint IDs, and all child layers have been
    // completed before their parents are evaluated.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<AtomicU8>(), bytes.len()) }
}

fn encode(value: Value) -> u8 {
    match value {
        Value::Win => WIN,
        Value::Loss => LOSS,
        Value::Draw => DRAW,
    }
}

fn decode(value: u8) -> Value {
    match value {
        WIN => Value::Win,
        LOSS => Value::Loss,
        DRAW => Value::Draw,
        _ => unreachable!("opening value has been resolved"),
    }
}

fn validate_threads(threads: usize) -> Result<(), OpeningError> {
    if threads == 0 {
        Err(OpeningError::InvalidThreadCount)
    } else {
        Ok(())
    }
}

fn join_results<T>(
    handles: Vec<std::thread::ScopedJoinHandle<'_, Result<T, OpeningError>>>,
) -> Result<Vec<T>, OpeningError> {
    handles
        .into_iter()
        .map(|handle| handle.join().map_err(|_| OpeningError::WorkerPanicked)?)
        .collect()
}

#[derive(Debug)]
pub enum OpeningError {
    InvalidThreadCount,
    WorkerPanicked,
    OpeningIdOutOfRange(u32),
    UnresolvedChild {
        parent: u32,
        child: u32,
    },
    AuditMismatch {
        id: u32,
        expected: Value,
        actual: Value,
    },
}

impl fmt::Display for OpeningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for OpeningError {}

#[cfg(test)]
mod tests {
    use super::*;

    struct ConstantPost(Value);

    impl PostValues for ConstantPost {
        fn value(&self, _: crate::ranking::PostOpeningId) -> Value {
            self.0
        }
    }

    #[test]
    fn final_opening_ply_obeys_minimax_against_post_values() {
        let range = opening_ply_range(5).unwrap();
        let opening = Vec::new();
        for (post, expected) in [
            (Value::Loss, Value::Win),
            (Value::Win, Value::Loss),
            (Value::Draw, Value::Draw),
        ] {
            let (actual, edges) =
                evaluate(range.start, &opening, &ConstantPost(post), Rules::default()).unwrap();
            assert_eq!(actual, expected);
            assert!(edges > 0);
        }
    }
}
