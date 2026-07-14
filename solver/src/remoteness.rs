//! Distance-to-termination enrichment for an audited W/L/D solution.
//!
//! Terminal losses have distance zero. A win is one plus the minimum distance
//! of its losing children; a nonterminal loss is one plus the maximum distance
//! of its winning children. Draws have no finite remoteness.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::graph::{for_each_opening_successor, OpeningChild};
use crate::opening::OpeningSolution;
use crate::ranking::{
    opening_ply_range, rank_opening, rank_post_opening, unrank_opening, OpeningId, PostOpeningId,
    LOCKED_OPENING_DOMAIN,
};
use crate::retrograde::{GameGraph, Solution, Value};
use crate::Rules;

const UNRESOLVED: u8 = 254;
pub const DRAW_CODE: u8 = 255;
pub const MAX_DISTANCE: u8 = 253;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemotenessStats {
    pub nodes: u64,
    pub wins: u64,
    pub losses: u64,
    pub draws: u64,
    pub terminal_losses: u64,
    pub dead_end_losses: u64,
    pub initialized_loss_edges: u64,
    pub maximum_distance: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemotenessWaveStats {
    pub distance: u8,
    pub input_frontier: u64,
    pub predecessor_edges: u64,
    pub resolved: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemotenessAuditStats {
    pub nodes: u64,
    pub decisive_nodes: u64,
    pub decisive_edges: u64,
    pub maximum_distance: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpeningRemotenessLayerStats {
    pub ply: u8,
    pub states: u64,
    pub edges: u64,
    pub maximum_distance: u8,
}

pub struct RemotenessTable {
    codes: Vec<u8>,
    stats: RemotenessStats,
}

impl RemotenessTable {
    pub fn node_count(&self) -> u32 {
        self.codes
            .len()
            .try_into()
            .expect("dense remoteness table fits u32 node IDs")
    }

    pub fn value(&self, node: u32) -> Value {
        value_from_code(self.codes[node as usize])
    }

    pub fn distance(&self, node: u32) -> Option<u8> {
        distance_from_code(self.codes[node as usize])
    }

    pub fn code(&self, node: u32) -> u8 {
        self.codes[node as usize]
    }

    pub const fn stats(&self) -> RemotenessStats {
        self.stats
    }

    pub fn codes(&self) -> &[u8] {
        &self.codes
    }
}

impl crate::opening::PostValues for RemotenessTable {
    fn value(&self, id: PostOpeningId) -> Value {
        self.value(id.get())
    }
}

pub struct OpeningRemotenessTable {
    codes: Vec<u8>,
    layers: [OpeningRemotenessLayerStats; 6],
}

impl OpeningRemotenessTable {
    pub fn value(&self, id: OpeningId) -> Value {
        value_from_code(self.codes[id.get() as usize])
    }

    pub fn distance(&self, id: OpeningId) -> Option<u8> {
        distance_from_code(self.codes[id.get() as usize])
    }

    pub fn code(&self, id: OpeningId) -> u8 {
        self.codes[id.get() as usize]
    }

    pub fn codes(&self) -> &[u8] {
        &self.codes
    }

    pub const fn layers(&self) -> &[OpeningRemotenessLayerStats; 6] {
        &self.layers
    }
}

pub fn solve_opening_parallel(
    opening: &OpeningSolution,
    post: &RemotenessTable,
    rules: Rules,
    threads: usize,
) -> Result<OpeningRemotenessTable, RemotenessError> {
    validate_threads(threads)?;
    let codes = vec![UNRESOLVED; LOCKED_OPENING_DOMAIN as usize];
    let atomic_codes = as_atomic_bytes(&codes);
    let mut layers = [OpeningRemotenessLayerStats::default(); 6];
    for ply in (0..=5_u8).rev() {
        let range = opening_ply_range(ply).expect("opening ply is in range");
        let length = (range.end - range.start) as usize;
        let chunk_size = length.div_ceil(threads).max(1);
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for first in (range.start..range.end).step_by(chunk_size) {
                let end = range.end.min(first.saturating_add(chunk_size as u32));
                handles.push(scope.spawn(move || {
                    solve_opening_chunk(first, end, opening, post, rules, atomic_codes)
                }));
            }
            join_results(handles)
        })?;
        let mut stats = OpeningRemotenessLayerStats {
            ply,
            ..OpeningRemotenessLayerStats::default()
        };
        for result in results {
            add_opening_stats(&mut stats, result);
        }
        layers[ply as usize] = stats;
    }
    Ok(OpeningRemotenessTable { codes, layers })
}

pub fn audit_opening_parallel(
    table: &OpeningRemotenessTable,
    opening: &OpeningSolution,
    post: &RemotenessTable,
    rules: Rules,
    threads: usize,
) -> Result<[OpeningRemotenessLayerStats; 6], RemotenessError> {
    validate_threads(threads)?;
    let mut layers = [OpeningRemotenessLayerStats::default(); 6];
    for ply in 0..=5_u8 {
        let range = opening_ply_range(ply).expect("opening ply is in range");
        let length = (range.end - range.start) as usize;
        let chunk_size = length.div_ceil(threads).max(1);
        let results =
            std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for first in (range.start..range.end).step_by(chunk_size) {
                    let end = range.end.min(first.saturating_add(chunk_size as u32));
                    handles.push(scope.spawn(move || {
                        audit_opening_chunk(first, end, table, opening, post, rules)
                    }));
                }
                join_results(handles)
            })?;
        let mut stats = OpeningRemotenessLayerStats {
            ply,
            ..OpeningRemotenessLayerStats::default()
        };
        for result in results {
            add_opening_stats(&mut stats, result);
        }
        layers[ply as usize] = stats;
    }
    Ok(layers)
}

pub fn solve_parallel(
    solution: &Solution,
    graph: &(impl GameGraph + Sync),
    threads: usize,
) -> Result<(RemotenessTable, Vec<RemotenessWaveStats>), RemotenessError> {
    validate_inputs(solution, graph, threads)?;
    let node_count = graph.node_count();
    if node_count == 0 {
        return Ok((
            RemotenessTable {
                codes: Vec::new(),
                stats: RemotenessStats::default(),
            },
            Vec::new(),
        ));
    }

    let mut codes = vec![UNRESOLVED; node_count as usize];
    let mut remaining = vec![0_u8; node_count as usize];
    let chunk_size = (node_count as usize).div_ceil(threads);
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for (chunk_index, (codes, remaining)) in codes
            .chunks_mut(chunk_size)
            .zip(remaining.chunks_mut(chunk_size))
            .enumerate()
        {
            let first = (chunk_index * chunk_size) as u32;
            handles.push(
                scope.spawn(move || initialize_chunk(solution, graph, first, codes, remaining)),
            );
        }
        join_results(handles)
    })?;

    let mut stats = RemotenessStats::default();
    let mut frontier = Vec::new();
    for result in results {
        add_stats(&mut stats, result.stats);
        frontier.extend(result.frontier);
    }
    frontier.sort_unstable();
    ensure_unique(&frontier)?;

    let atomic_codes = as_atomic_bytes(&codes);
    let atomic_remaining = as_atomic_bytes(&remaining);
    let mut waves = Vec::new();
    let mut distance = 0_u8;
    while !frontier.is_empty() {
        let next_distance = distance
            .checked_add(1)
            .filter(|next| *next <= MAX_DISTANCE)
            .ok_or(RemotenessError::DistanceOverflow { distance })?;
        let chunk_size = frontier.len().div_ceil(threads);
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for chunk in frontier.chunks(chunk_size) {
                handles.push(scope.spawn(move || {
                    process_chunk(
                        solution,
                        graph,
                        chunk,
                        distance,
                        next_distance,
                        atomic_codes,
                        atomic_remaining,
                    )
                }));
            }
            join_results(handles)
        })?;

        let mut wave = RemotenessWaveStats {
            distance,
            input_frontier: frontier.len() as u64,
            ..RemotenessWaveStats::default()
        };
        let mut next = Vec::new();
        for result in results {
            wave.predecessor_edges += result.stats.predecessor_edges;
            wave.resolved += result.stats.resolved;
            next.extend(result.frontier);
        }
        next.sort_unstable();
        ensure_unique(&next)?;
        waves.push(wave);
        frontier = next;
        distance = next_distance;
    }

    drop(remaining);
    let stats = validate_codes(solution, &codes, stats)?;
    Ok((RemotenessTable { codes, stats }, waves))
}

pub fn audit_parallel(
    table: &RemotenessTable,
    solution: &Solution,
    graph: &(impl GameGraph + Sync),
    threads: usize,
) -> Result<RemotenessAuditStats, RemotenessError> {
    validate_inputs(solution, graph, threads)?;
    if table.node_count() != graph.node_count() {
        return Err(RemotenessError::NodeCountMismatch {
            expected: graph.node_count() as u64,
            actual: table.node_count() as u64,
        });
    }
    let node_count = graph.node_count();
    if node_count == 0 {
        return Ok(RemotenessAuditStats::default());
    }
    let chunk_size = (node_count as usize).div_ceil(threads);
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for first in (0..node_count).step_by(chunk_size) {
            let end = node_count.min(first.saturating_add(chunk_size as u32));
            handles.push(scope.spawn(move || audit_chunk(table, solution, graph, first, end)));
        }
        join_results(handles)
    })?;
    let mut stats = RemotenessAuditStats::default();
    for result in results {
        stats.nodes += result.nodes;
        stats.decisive_nodes += result.decisive_nodes;
        stats.decisive_edges += result.decisive_edges;
        stats.maximum_distance = stats.maximum_distance.max(result.maximum_distance);
    }
    Ok(stats)
}

fn solve_opening_chunk(
    first: u32,
    end: u32,
    opening: &OpeningSolution,
    post: &RemotenessTable,
    rules: Rules,
    codes: &[AtomicU8],
) -> Result<OpeningRemotenessLayerStats, RemotenessError> {
    let mut stats = OpeningRemotenessLayerStats::default();
    for raw in first..end {
        let id = OpeningId::new(raw).ok_or(RemotenessError::OpeningIdOutOfRange(raw))?;
        let mut children = ChildDistances::default();
        for_each_opening_successor(id, rules, |child| {
            let (value, distance) = match child {
                OpeningChild::Opening(child) => {
                    let code = codes[child.get() as usize].load(Ordering::Relaxed);
                    if code == UNRESOLVED {
                        children.unresolved_child = Some(child.get() as u64);
                        return;
                    }
                    (value_from_code(code), distance_from_code(code))
                }
                OpeningChild::PostOpening(child) => {
                    (post.value(child.get()), post.distance(child.get()))
                }
            };
            children.observe(value, distance);
        });
        if let Some(child) = children.unresolved_child {
            return Err(RemotenessError::UnresolvedOpeningChild { parent: raw, child });
        }
        let code = children.finish(raw, opening.value(id))?;
        codes[raw as usize].store(code, Ordering::Relaxed);
        accumulate_opening(&mut stats, code, children.edges);
    }
    Ok(stats)
}

fn audit_opening_chunk(
    first: u32,
    end: u32,
    table: &OpeningRemotenessTable,
    opening: &OpeningSolution,
    post: &RemotenessTable,
    rules: Rules,
) -> Result<OpeningRemotenessLayerStats, RemotenessError> {
    let mut stats = OpeningRemotenessLayerStats::default();
    for raw in first..end {
        let id = OpeningId::new(raw).ok_or(RemotenessError::OpeningIdOutOfRange(raw))?;
        let position = unrank_opening(id);
        let actions = position.legal_moves(rules);
        let mut children = ChildDistances::default();
        for action in actions {
            let child = position
                .play(action, rules)
                .expect("reference opening action remains legal");
            let (value, distance) = if child.opening_complete() {
                let child =
                    rank_post_opening(&child).expect("reference unlocked child is rankable");
                (post.value(child.get()), post.distance(child.get()))
            } else {
                let child = rank_opening(&child).expect("reference locked child is rankable");
                (table.value(child), table.distance(child))
            };
            children.observe(value, distance);
        }
        let expected = children.finish(raw, opening.value(id))?;
        let actual = table.code(id);
        if actual != expected {
            return Err(RemotenessError::OpeningAuditMismatch {
                id: raw,
                expected,
                actual,
            });
        }
        accumulate_opening(&mut stats, actual, children.edges);
    }
    Ok(stats)
}

#[derive(Default)]
struct ChildDistances {
    edges: u64,
    has_loss: bool,
    all_win: bool,
    has_draw: bool,
    minimum_loss: Option<u8>,
    maximum_win: Option<u8>,
    unresolved_child: Option<u64>,
}

impl ChildDistances {
    fn observe(&mut self, value: Value, distance: Option<u8>) {
        if self.edges == 0 {
            self.all_win = true;
        }
        self.edges += 1;
        self.all_win &= value == Value::Win;
        match value {
            Value::Loss => {
                self.has_loss = true;
                let distance = distance.expect("loss has finite remoteness");
                self.minimum_loss = Some(
                    self.minimum_loss
                        .map_or(distance, |current| current.min(distance)),
                );
            }
            Value::Win => {
                let distance = distance.expect("win has finite remoteness");
                self.maximum_win = Some(
                    self.maximum_win
                        .map_or(distance, |current| current.max(distance)),
                );
            }
            Value::Draw => self.has_draw = true,
        }
    }

    fn finish(&self, node: u32, parent: Value) -> Result<u8, RemotenessError> {
        match parent {
            Value::Win => {
                let distance =
                    self.minimum_loss
                        .ok_or(RemotenessError::OpeningValueEquationMismatch {
                            node,
                            value: parent,
                        })?;
                next_distance(distance)
            }
            Value::Loss if self.edges == 0 => Ok(0),
            Value::Loss if self.all_win => {
                next_distance(self.maximum_win.expect("nonempty all-win children"))
            }
            Value::Draw if !self.has_loss && self.has_draw => Ok(DRAW_CODE),
            _ => Err(RemotenessError::OpeningValueEquationMismatch {
                node,
                value: parent,
            }),
        }
    }
}

fn next_distance(distance: u8) -> Result<u8, RemotenessError> {
    distance
        .checked_add(1)
        .filter(|next| *next <= MAX_DISTANCE)
        .ok_or(RemotenessError::DistanceOverflow { distance })
}

fn accumulate_opening(stats: &mut OpeningRemotenessLayerStats, code: u8, edges: u64) {
    stats.states += 1;
    stats.edges += edges;
    if let Some(distance) = distance_from_code(code) {
        stats.maximum_distance = stats.maximum_distance.max(distance);
    }
}

fn add_opening_stats(
    target: &mut OpeningRemotenessLayerStats,
    source: OpeningRemotenessLayerStats,
) {
    target.states += source.states;
    target.edges += source.edges;
    target.maximum_distance = target.maximum_distance.max(source.maximum_distance);
}

struct InitializationChunk {
    stats: RemotenessStats,
    frontier: Vec<u32>,
}

fn initialize_chunk(
    solution: &Solution,
    graph: &impl GameGraph,
    first: u32,
    codes: &mut [u8],
    remaining: &mut [u8],
) -> Result<InitializationChunk, RemotenessError> {
    let node_count = graph.node_count();
    let mut result = InitializationChunk {
        stats: RemotenessStats::default(),
        frontier: Vec::new(),
    };
    for offset in 0..codes.len() {
        let node = first + offset as u32;
        result.stats.nodes += 1;
        match solution.value(node) {
            Value::Draw => {
                codes[offset] = DRAW_CODE;
                result.stats.draws += 1;
            }
            Value::Win => result.stats.wins += 1,
            Value::Loss => {
                result.stats.losses += 1;
                if graph.is_terminal_loss(node) {
                    codes[offset] = 0;
                    result.stats.terminal_losses += 1;
                    result.frontier.push(node);
                    continue;
                }

                let mut degree = 0_u16;
                let mut bad_child = None;
                let mut non_win_child = None;
                graph.for_each_successor(node, |child| {
                    if child >= node_count {
                        bad_child = Some(child);
                    } else {
                        degree += 1;
                        if solution.value(child) != Value::Win {
                            non_win_child = Some(child);
                        }
                    }
                });
                if let Some(child) = bad_child {
                    return Err(RemotenessError::EdgeOutOfRange {
                        from: node,
                        to: child,
                    });
                }
                if let Some(child) = non_win_child {
                    return Err(RemotenessError::ValueEquationMismatch {
                        node,
                        child,
                        parent_value: Value::Loss,
                        child_value: solution.value(child),
                    });
                }
                if degree > u8::MAX as u16 {
                    return Err(RemotenessError::DegreeOverflow { node, degree });
                }
                result.stats.initialized_loss_edges += degree as u64;
                if degree == 0 {
                    codes[offset] = 0;
                    result.stats.dead_end_losses += 1;
                    result.frontier.push(node);
                } else {
                    remaining[offset] = degree as u8;
                }
            }
        }
    }
    Ok(result)
}

struct WaveChunk {
    stats: RemotenessWaveStats,
    frontier: Vec<u32>,
}

#[allow(clippy::too_many_arguments)]
fn process_chunk(
    solution: &Solution,
    graph: &impl GameGraph,
    children: &[u32],
    distance: u8,
    next_distance: u8,
    codes: &[AtomicU8],
    remaining: &[AtomicU8],
) -> Result<WaveChunk, RemotenessError> {
    let node_count = graph.node_count();
    let mut result = WaveChunk {
        stats: RemotenessWaveStats::default(),
        frontier: Vec::new(),
    };
    for &child in children {
        let child_code = codes[child as usize].load(Ordering::Relaxed);
        if child_code != distance {
            return Err(RemotenessError::InvalidFrontierDistance {
                node: child,
                expected: distance,
                actual: child_code,
            });
        }
        let child_value = solution.value(child);
        let mut error = None;
        graph.for_each_predecessor(child, |parent| {
            if error.is_some() {
                return;
            }
            if parent >= node_count {
                error = Some(RemotenessError::EdgeOutOfRange {
                    from: parent,
                    to: child,
                });
                return;
            }
            result.stats.predecessor_edges += 1;
            let index = parent as usize;
            match (child_value, solution.value(parent)) {
                (Value::Loss, Value::Win) => {
                    if codes[index]
                        .compare_exchange(
                            UNRESOLVED,
                            next_distance,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        result.frontier.push(parent);
                        result.stats.resolved += 1;
                    }
                }
                (Value::Loss, parent_value) => {
                    error = Some(RemotenessError::ValueEquationMismatch {
                        node: parent,
                        child,
                        parent_value,
                        child_value,
                    });
                }
                (Value::Win, Value::Loss) => {
                    if codes[index].load(Ordering::Relaxed) != UNRESOLVED {
                        return;
                    }
                    let previous = remaining[index].fetch_sub(1, Ordering::Relaxed);
                    if previous == 0 {
                        remaining[index].fetch_add(1, Ordering::Relaxed);
                        error = Some(RemotenessError::CounterUnderflow { node: parent });
                        return;
                    }
                    if previous == 1 {
                        if codes[index]
                            .compare_exchange(
                                UNRESOLVED,
                                next_distance,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            )
                            .is_err()
                        {
                            error = Some(RemotenessError::DuplicateResolution { node: parent });
                            return;
                        }
                        result.frontier.push(parent);
                        result.stats.resolved += 1;
                    }
                }
                (Value::Win, Value::Win | Value::Draw) => {}
                (Value::Draw, _) => {
                    error = Some(RemotenessError::DrawInFrontier { node: child });
                }
            }
        });
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(result)
}

fn audit_chunk(
    table: &RemotenessTable,
    solution: &Solution,
    graph: &impl GameGraph,
    first: u32,
    end: u32,
) -> Result<RemotenessAuditStats, RemotenessError> {
    let node_count = graph.node_count();
    let mut stats = RemotenessAuditStats::default();
    for node in first..end {
        stats.nodes += 1;
        let actual_value = table.value(node);
        let solution_value = solution.value(node);
        if actual_value != solution_value {
            return Err(RemotenessError::EncodedValueMismatch {
                node,
                expected: solution_value,
                actual: actual_value,
            });
        }
        let Some(actual_distance) = table.distance(node) else {
            continue;
        };
        stats.decisive_nodes += 1;
        stats.maximum_distance = stats.maximum_distance.max(actual_distance);

        let terminal = graph.is_terminal_loss(node);
        let mut degree = 0_u16;
        let mut bad_child = None;
        let mut candidate = match solution_value {
            Value::Win => u8::MAX,
            Value::Loss => 0,
            Value::Draw => unreachable!("draws have no finite distance"),
        };
        graph.for_each_successor(node, |child| {
            if child >= node_count {
                bad_child = Some(child);
                return;
            }
            degree += 1;
            match solution_value {
                Value::Win if table.value(child) == Value::Loss => {
                    candidate = candidate.min(table.distance(child).expect("loss is decisive"));
                }
                Value::Loss if table.value(child) == Value::Win => {
                    candidate = candidate.max(table.distance(child).expect("win is decisive"));
                }
                _ => {}
            }
        });
        if let Some(child) = bad_child {
            return Err(RemotenessError::EdgeOutOfRange {
                from: node,
                to: child,
            });
        }
        stats.decisive_edges += degree as u64;
        let expected = if terminal || degree == 0 {
            0
        } else {
            if solution_value == Value::Win && candidate == u8::MAX {
                return Err(RemotenessError::MissingDecisiveChild { node });
            }
            candidate
                .checked_add(1)
                .filter(|distance| *distance <= MAX_DISTANCE)
                .ok_or(RemotenessError::DistanceOverflow {
                    distance: candidate,
                })?
        };
        if actual_distance != expected {
            return Err(RemotenessError::AuditMismatch {
                node,
                expected,
                actual: actual_distance,
            });
        }
    }
    Ok(stats)
}

fn validate_inputs(
    solution: &Solution,
    graph: &impl GameGraph,
    threads: usize,
) -> Result<(), RemotenessError> {
    validate_threads(threads)?;
    if solution.node_count() != graph.node_count() {
        return Err(RemotenessError::NodeCountMismatch {
            expected: graph.node_count() as u64,
            actual: solution.node_count() as u64,
        });
    }
    Ok(())
}

fn validate_threads(threads: usize) -> Result<(), RemotenessError> {
    if threads == 0 {
        Err(RemotenessError::InvalidThreadCount)
    } else {
        Ok(())
    }
}

fn validate_codes(
    solution: &Solution,
    codes: &[u8],
    mut stats: RemotenessStats,
) -> Result<RemotenessStats, RemotenessError> {
    for (node, &code) in codes.iter().enumerate() {
        if code == UNRESOLVED {
            return Err(RemotenessError::UnresolvedNode { node: node as u32 });
        }
        let actual = value_from_code(code);
        let expected = solution.value(node as u32);
        if actual != expected {
            return Err(RemotenessError::EncodedValueMismatch {
                node: node as u32,
                expected,
                actual,
            });
        }
        if let Some(distance) = distance_from_code(code) {
            stats.maximum_distance = stats.maximum_distance.max(distance);
        }
    }
    Ok(stats)
}

fn add_stats(target: &mut RemotenessStats, source: RemotenessStats) {
    target.nodes += source.nodes;
    target.wins += source.wins;
    target.losses += source.losses;
    target.draws += source.draws;
    target.terminal_losses += source.terminal_losses;
    target.dead_end_losses += source.dead_end_losses;
    target.initialized_loss_edges += source.initialized_loss_edges;
}

fn value_from_code(code: u8) -> Value {
    match code {
        DRAW_CODE => Value::Draw,
        UNRESOLVED => unreachable!("unresolved remoteness is not a table value"),
        distance if distance.is_multiple_of(2) => Value::Loss,
        _ => Value::Win,
    }
}

fn distance_from_code(code: u8) -> Option<u8> {
    match code {
        DRAW_CODE => None,
        UNRESOLVED => unreachable!("unresolved remoteness is not a table value"),
        distance => Some(distance),
    }
}

fn as_atomic_bytes(bytes: &[u8]) -> &[AtomicU8] {
    assert_eq!(std::mem::size_of::<AtomicU8>(), std::mem::size_of::<u8>());
    assert_eq!(std::mem::align_of::<AtomicU8>(), std::mem::align_of::<u8>());
    // SAFETY: the backing arrays are only accessed through these atomic
    // references until propagation finishes.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<AtomicU8>(), bytes.len()) }
}

fn ensure_unique(frontier: &[u32]) -> Result<(), RemotenessError> {
    if let Some(window) = frontier.windows(2).find(|window| window[0] == window[1]) {
        return Err(RemotenessError::DuplicateFrontierNode(window[0]));
    }
    Ok(())
}

fn join_results<T>(
    handles: Vec<std::thread::ScopedJoinHandle<'_, Result<T, RemotenessError>>>,
) -> Result<Vec<T>, RemotenessError> {
    handles
        .into_iter()
        .map(|handle| handle.join().map_err(|_| RemotenessError::WorkerPanicked)?)
        .collect()
}

#[derive(Debug)]
pub enum RemotenessError {
    InvalidThreadCount,
    WorkerPanicked,
    NodeCountMismatch {
        expected: u64,
        actual: u64,
    },
    EdgeOutOfRange {
        from: u32,
        to: u32,
    },
    DegreeOverflow {
        node: u32,
        degree: u16,
    },
    CounterUnderflow {
        node: u32,
    },
    DuplicateResolution {
        node: u32,
    },
    DuplicateFrontierNode(u32),
    InvalidFrontierDistance {
        node: u32,
        expected: u8,
        actual: u8,
    },
    DrawInFrontier {
        node: u32,
    },
    ValueEquationMismatch {
        node: u32,
        child: u32,
        parent_value: Value,
        child_value: Value,
    },
    EncodedValueMismatch {
        node: u32,
        expected: Value,
        actual: Value,
    },
    MissingDecisiveChild {
        node: u32,
    },
    UnresolvedNode {
        node: u32,
    },
    DistanceOverflow {
        distance: u8,
    },
    AuditMismatch {
        node: u32,
        expected: u8,
        actual: u8,
    },
    OpeningIdOutOfRange(u32),
    UnresolvedOpeningChild {
        parent: u32,
        child: u64,
    },
    OpeningValueEquationMismatch {
        node: u32,
        value: Value,
    },
    OpeningAuditMismatch {
        id: u32,
        expected: u8,
        actual: u8,
    },
}

impl fmt::Display for RemotenessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RemotenessError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrograde;

    struct Graph {
        successors: Vec<Vec<u32>>,
    }

    impl GameGraph for Graph {
        fn node_count(&self) -> u32 {
            self.successors.len() as u32
        }

        fn is_terminal_loss(&self, node: u32) -> bool {
            node == 0
        }

        fn for_each_successor(&self, node: u32, mut emit: impl FnMut(u32)) {
            for &child in &self.successors[node as usize] {
                emit(child);
            }
        }

        fn for_each_predecessor(&self, node: u32, mut emit: impl FnMut(u32)) {
            for (parent, children) in self.successors.iter().enumerate() {
                if children.contains(&node) {
                    emit(parent as u32);
                }
            }
        }
    }

    #[test]
    fn distances_follow_minimum_wins_and_maximum_losses() {
        let graph = Graph {
            successors: vec![vec![], vec![0], vec![1], vec![3], vec![2, 3]],
        };
        let solution = retrograde::solve(&graph).unwrap();
        let (table, waves) = solve_parallel(&solution, &graph, 3).unwrap();
        assert_eq!(
            (0..5).map(|node| table.code(node)).collect::<Vec<_>>(),
            vec![0, 1, 2, DRAW_CODE, 3]
        );
        assert_eq!(table.stats().maximum_distance, 3);
        assert_eq!(
            waves.iter().map(|wave| wave.distance).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let audit = audit_parallel(&table, &solution, &graph, 2).unwrap();
        assert_eq!(audit.nodes, 5);
        assert_eq!(audit.decisive_nodes, 4);
        assert_eq!(audit.maximum_distance, 3);
    }

    #[test]
    fn thread_count_does_not_change_codes() {
        let graph = Graph {
            successors: vec![vec![], vec![0], vec![1], vec![0, 2], vec![3]],
        };
        let solution = retrograde::solve(&graph).unwrap();
        let (serial, _) = solve_parallel(&solution, &graph, 1).unwrap();
        let (parallel, _) = solve_parallel(&solution, &graph, 4).unwrap();
        assert_eq!(serial.codes(), parallel.codes());
    }

    #[test]
    fn opening_distance_reducer_uses_minimum_win_and_maximum_loss() {
        let mut winning = ChildDistances::default();
        winning.observe(Value::Loss, Some(8));
        winning.observe(Value::Loss, Some(2));
        winning.observe(Value::Draw, None);
        assert_eq!(winning.finish(0, Value::Win).unwrap(), 3);

        let mut losing = ChildDistances::default();
        losing.observe(Value::Win, Some(1));
        losing.observe(Value::Win, Some(9));
        assert_eq!(losing.finish(0, Value::Loss).unwrap(), 10);

        let mut drawing = ChildDistances::default();
        drawing.observe(Value::Win, Some(3));
        drawing.observe(Value::Draw, None);
        assert_eq!(drawing.finish(0, Value::Draw).unwrap(), DRAW_CODE);
    }
}
