//! Deterministic wave-parallel retrograde analysis over dense `u32` IDs.

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::checkpoint::{self, Checkpoint, CheckpointError};
use crate::retrograde::{GameGraph, Solution};

const UNKNOWN: u8 = 0;
const WIN: u8 = 1;
const LOSS: u8 = 2;

pub struct ParallelState {
    rules_tag: u32,
    wave: u64,
    values: Vec<u8>,
    remaining: Vec<u8>,
    frontier: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InitializationStats {
    pub nodes: u64,
    pub edges: u64,
    pub terminal_losses: u64,
    pub dead_end_losses: u64,
    pub maximum_degree: u8,
    pub frontier: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WaveStats {
    pub wave: u64,
    pub input_frontier: u64,
    pub predecessor_edges: u64,
    pub skipped_resolved: u64,
    pub counter_decrements: u64,
    pub resolved_wins: u64,
    pub resolved_losses: u64,
    pub output_frontier: u64,
}

impl ParallelState {
    pub fn initialize(
        graph: &(impl GameGraph + Sync),
        rules_tag: u32,
        threads: usize,
    ) -> Result<(Self, InitializationStats), ParallelError> {
        validate_threads(threads)?;
        let node_count = graph.node_count();
        let mut values = vec![UNKNOWN; node_count as usize];
        let mut remaining = vec![0; node_count as usize];
        if node_count == 0 {
            return Ok((
                Self {
                    rules_tag,
                    wave: 0,
                    values,
                    remaining,
                    frontier: Vec::new(),
                },
                InitializationStats::default(),
            ));
        }

        let chunk_size = (node_count as usize).div_ceil(threads);
        let results =
            std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for (chunk_index, (value_chunk, remaining_chunk)) in values
                    .chunks_mut(chunk_size)
                    .zip(remaining.chunks_mut(chunk_size))
                    .enumerate()
                {
                    let first = (chunk_index * chunk_size) as u32;
                    handles.push(scope.spawn(move || {
                        initialize_chunk(graph, first, value_chunk, remaining_chunk)
                    }));
                }
                join_results(handles)
            })?;

        let mut stats = InitializationStats::default();
        let mut frontier = Vec::new();
        for result in results {
            stats.nodes += result.stats.nodes;
            stats.edges += result.stats.edges;
            stats.terminal_losses += result.stats.terminal_losses;
            stats.dead_end_losses += result.stats.dead_end_losses;
            stats.maximum_degree = stats.maximum_degree.max(result.stats.maximum_degree);
            frontier.extend(result.frontier);
        }
        frontier.sort_unstable();
        ensure_unique(&frontier)?;
        stats.frontier = frontier.len() as u64;

        Ok((
            Self {
                rules_tag,
                wave: 0,
                values,
                remaining,
                frontier,
            },
            stats,
        ))
    }

    pub fn load(
        path: impl AsRef<Path>,
        graph: &impl GameGraph,
        rules_tag: u32,
    ) -> Result<Self, ParallelError> {
        let checkpoint = Checkpoint::load(path, graph.node_count(), rules_tag)?;
        let mut state = Self {
            rules_tag: checkpoint.rules_tag,
            wave: checkpoint.wave,
            values: checkpoint.values,
            remaining: checkpoint.remaining,
            frontier: checkpoint.frontier,
        };
        state.frontier.sort_unstable();
        ensure_unique(&state.frontier)?;
        Ok(state)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ParallelError> {
        checkpoint::save_atomic(
            path,
            self.rules_tag,
            self.wave,
            &self.values,
            &self.remaining,
            &self.frontier,
        )?;
        Ok(())
    }

    pub const fn wave(&self) -> u64 {
        self.wave
    }

    pub fn frontier(&self) -> &[u32] {
        &self.frontier
    }

    pub fn encoded_value(&self, node: u32) -> u8 {
        self.values[node as usize]
    }

    pub fn remaining_count(&self, node: u32) -> u8 {
        self.remaining[node as usize]
    }

    pub fn run_wave(
        &mut self,
        graph: &(impl GameGraph + Sync),
        threads: usize,
    ) -> Result<WaveStats, ParallelError> {
        validate_threads(threads)?;
        if graph.node_count() as usize != self.values.len() {
            return Err(ParallelError::NodeCountMismatch {
                expected: self.values.len() as u64,
                actual: graph.node_count() as u64,
            });
        }
        let current_wave = self.wave;
        if self.frontier.is_empty() {
            return Ok(WaveStats {
                wave: current_wave,
                ..WaveStats::default()
            });
        }

        let values = as_atomic_bytes(&self.values);
        let remaining = as_atomic_bytes(&self.remaining);
        let chunk_size = self.frontier.len().div_ceil(threads);
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for chunk in self.frontier.chunks(chunk_size) {
                handles.push(scope.spawn(move || process_chunk(graph, chunk, values, remaining)));
            }
            join_results(handles)
        })?;

        let mut stats = WaveStats {
            wave: current_wave,
            input_frontier: self.frontier.len() as u64,
            ..WaveStats::default()
        };
        let mut next = Vec::new();
        for result in results {
            stats.predecessor_edges += result.stats.predecessor_edges;
            stats.skipped_resolved += result.stats.skipped_resolved;
            stats.counter_decrements += result.stats.counter_decrements;
            stats.resolved_wins += result.stats.resolved_wins;
            stats.resolved_losses += result.stats.resolved_losses;
            next.extend(result.frontier);
        }
        next.sort_unstable();
        ensure_unique(&next)?;
        stats.output_frontier = next.len() as u64;
        self.frontier = next;
        self.wave += 1;
        Ok(stats)
    }

    pub fn finish(self) -> Result<Solution, ParallelError> {
        if !self.frontier.is_empty() {
            return Err(ParallelError::FixpointNotReached {
                wave: self.wave,
                frontier: self.frontier.len() as u64,
            });
        }
        Ok(Solution::from_fixpoint(self.values))
    }
}

struct InitializationChunk {
    stats: InitializationStats,
    frontier: Vec<u32>,
}

fn initialize_chunk(
    graph: &impl GameGraph,
    first: u32,
    values: &mut [u8],
    remaining: &mut [u8],
) -> Result<InitializationChunk, ParallelError> {
    let node_count = graph.node_count();
    let mut result = InitializationChunk {
        stats: InitializationStats::default(),
        frontier: Vec::new(),
    };
    for offset in 0..values.len() {
        let node = first + offset as u32;
        let mut degree = 0_u16;
        let mut bad_child = None;
        graph.for_each_successor(node, |child| {
            if child >= node_count {
                bad_child = Some(child);
            } else {
                degree += 1;
            }
        });
        if let Some(child) = bad_child {
            return Err(ParallelError::EdgeOutOfRange {
                from: node,
                to: child,
            });
        }
        if degree > u8::MAX as u16 {
            return Err(ParallelError::DegreeOverflow { node, degree });
        }
        let terminal = graph.is_terminal_loss(node);
        if terminal && degree != 0 {
            return Err(ParallelError::TerminalHasSuccessors { node, degree });
        }

        values[offset] = if terminal || degree == 0 {
            LOSS
        } else {
            UNKNOWN
        };
        remaining[offset] = degree as u8;
        result.stats.nodes += 1;
        result.stats.edges += degree as u64;
        result.stats.maximum_degree = result.stats.maximum_degree.max(degree as u8);
        if terminal {
            result.stats.terminal_losses += 1;
            result.frontier.push(node);
        } else if degree == 0 {
            result.stats.dead_end_losses += 1;
            result.frontier.push(node);
        }
    }
    Ok(result)
}

struct WaveChunk {
    stats: WaveStats,
    frontier: Vec<u32>,
}

fn process_chunk(
    graph: &impl GameGraph,
    children: &[u32],
    values: &[AtomicU8],
    remaining: &[AtomicU8],
) -> Result<WaveChunk, ParallelError> {
    let node_count = graph.node_count();
    let mut result = WaveChunk {
        stats: WaveStats::default(),
        frontier: Vec::new(),
    };
    for &child in children {
        let child_value = values[child as usize].load(Ordering::Relaxed);
        if child_value != WIN && child_value != LOSS {
            return Err(ParallelError::InvalidFrontierValue {
                node: child,
                value: child_value,
            });
        }

        let mut error = None;
        graph.for_each_predecessor(child, |parent| {
            if error.is_some() {
                return;
            }
            if parent >= node_count {
                error = Some(ParallelError::EdgeOutOfRange {
                    from: parent,
                    to: child,
                });
                return;
            }
            result.stats.predecessor_edges += 1;
            let index = parent as usize;
            if child_value == LOSS {
                if values[index]
                    .compare_exchange(UNKNOWN, WIN, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    result.frontier.push(parent);
                    result.stats.resolved_wins += 1;
                } else {
                    result.stats.skipped_resolved += 1;
                }
                return;
            }

            if values[index].load(Ordering::Relaxed) != UNKNOWN {
                result.stats.skipped_resolved += 1;
                return;
            }
            let previous = remaining[index].fetch_sub(1, Ordering::Relaxed);
            if previous == 0 {
                remaining[index].fetch_add(1, Ordering::Relaxed);
                error = Some(ParallelError::CounterUnderflow { node: parent });
                return;
            }
            result.stats.counter_decrements += 1;
            if previous == 1
                && values[index]
                    .compare_exchange(UNKNOWN, LOSS, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                result.frontier.push(parent);
                result.stats.resolved_losses += 1;
            }
        });
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(result)
}

fn as_atomic_bytes(bytes: &[u8]) -> &[AtomicU8] {
    assert_eq!(std::mem::size_of::<AtomicU8>(), std::mem::size_of::<u8>());
    assert_eq!(std::mem::align_of::<AtomicU8>(), std::mem::align_of::<u8>());
    // SAFETY: the asserted byte layout matches. The wave owns the only access
    // to the backing arrays, and every worker uses these atomic references.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<AtomicU8>(), bytes.len()) }
}

fn ensure_unique(frontier: &[u32]) -> Result<(), ParallelError> {
    if let Some(window) = frontier.windows(2).find(|window| window[0] == window[1]) {
        return Err(ParallelError::DuplicateFrontierNode(window[0]));
    }
    Ok(())
}

fn validate_threads(threads: usize) -> Result<(), ParallelError> {
    if threads == 0 {
        Err(ParallelError::InvalidThreadCount)
    } else {
        Ok(())
    }
}

fn join_results<T>(
    handles: Vec<std::thread::ScopedJoinHandle<'_, Result<T, ParallelError>>>,
) -> Result<Vec<T>, ParallelError> {
    handles
        .into_iter()
        .map(|handle| handle.join().map_err(|_| ParallelError::WorkerPanicked)?)
        .collect()
}

#[derive(Debug)]
pub enum ParallelError {
    Checkpoint(CheckpointError),
    InvalidThreadCount,
    WorkerPanicked,
    NodeCountMismatch { expected: u64, actual: u64 },
    EdgeOutOfRange { from: u32, to: u32 },
    DegreeOverflow { node: u32, degree: u16 },
    TerminalHasSuccessors { node: u32, degree: u16 },
    CounterUnderflow { node: u32 },
    InvalidFrontierValue { node: u32, value: u8 },
    DuplicateFrontierNode(u32),
    FixpointNotReached { wave: u64, frontier: u64 },
}

impl fmt::Display for ParallelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ParallelError {}

impl From<CheckpointError> for ParallelError {
    fn from(error: CheckpointError) -> Self {
        Self::Checkpoint(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrograde;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct ExplicitGraph {
        successors: Vec<Vec<u32>>,
        predecessors: Vec<Vec<u32>>,
        terminals: Vec<bool>,
    }

    impl ExplicitGraph {
        fn new(successors: Vec<Vec<u32>>, terminals: &[u32]) -> Self {
            let mut predecessors = vec![Vec::new(); successors.len()];
            for (parent, children) in successors.iter().enumerate() {
                for &child in children {
                    predecessors[child as usize].push(parent as u32);
                }
            }
            let mut terminal_flags = vec![false; successors.len()];
            for &terminal in terminals {
                terminal_flags[terminal as usize] = true;
            }
            Self {
                successors,
                predecessors,
                terminals: terminal_flags,
            }
        }
    }

    impl GameGraph for ExplicitGraph {
        fn node_count(&self) -> u32 {
            self.successors.len() as u32
        }

        fn is_terminal_loss(&self, node: u32) -> bool {
            self.terminals[node as usize]
        }

        fn for_each_successor(&self, node: u32, mut emit: impl FnMut(u32)) {
            self.successors[node as usize]
                .iter()
                .copied()
                .for_each(&mut emit);
        }

        fn for_each_predecessor(&self, node: u32, mut emit: impl FnMut(u32)) {
            self.predecessors[node as usize]
                .iter()
                .copied()
                .for_each(&mut emit);
        }
    }

    #[test]
    fn parallel_waves_match_reference_solver_on_random_graphs() {
        let mut random = 0xbb67_ae85_84ca_a73b_u64;
        for nodes in 1..=30_u32 {
            for _ in 0..50 {
                let graph = random_graph(nodes, &mut random);
                let reference = retrograde::solve(&graph).unwrap();
                for threads in [1, 4] {
                    let (mut state, _) = ParallelState::initialize(&graph, 7, threads).unwrap();
                    while !state.frontier().is_empty() {
                        state.run_wave(&graph, threads).unwrap();
                    }
                    let solution = state.finish().unwrap();
                    for node in 0..nodes {
                        assert_eq!(solution.value(node), reference.value(node));
                    }
                    solution.audit(&graph).unwrap();
                }
            }
        }
    }

    #[test]
    fn wave_boundaries_are_deterministic_across_thread_counts() {
        let graph = ExplicitGraph::new(
            vec![vec![1, 2], vec![3], vec![3, 4], vec![5], vec![4], vec![]],
            &[5],
        );
        let (mut one, _) = ParallelState::initialize(&graph, 9, 1).unwrap();
        let (mut four, _) = ParallelState::initialize(&graph, 9, 4).unwrap();
        while !one.frontier().is_empty() {
            assert_eq!(one.frontier(), four.frontier());
            one.run_wave(&graph, 1).unwrap();
            four.run_wave(&graph, 4).unwrap();
            assert_eq!(one.values, four.values);
            assert_eq!(one.remaining, four.remaining);
        }
        assert!(four.frontier().is_empty());
    }

    #[test]
    fn checkpoint_restart_matches_uninterrupted_waves() {
        let graph = ExplicitGraph::new(
            vec![vec![1], vec![2, 3], vec![4], vec![4], vec![5], vec![]],
            &[5],
        );
        let path = test_path();
        let (mut uninterrupted, _) = ParallelState::initialize(&graph, 23, 3).unwrap();
        uninterrupted.run_wave(&graph, 3).unwrap();
        uninterrupted.save(&path).unwrap();

        let mut restarted = ParallelState::load(&path, &graph, 23).unwrap();
        while !uninterrupted.frontier().is_empty() {
            uninterrupted.run_wave(&graph, 3).unwrap();
        }
        while !restarted.frontier().is_empty() {
            restarted.run_wave(&graph, 2).unwrap();
        }
        assert_eq!(uninterrupted.values, restarted.values);
        assert_eq!(uninterrupted.remaining, restarted.remaining);
        assert_eq!(uninterrupted.wave, restarted.wave);
        fs::remove_file(path).unwrap();
    }

    fn random_graph(nodes: u32, random: &mut u64) -> ExplicitGraph {
        let mut successors = vec![Vec::new(); nodes as usize];
        for parent in 0..nodes {
            *random = next_random(*random);
            let degree = (*random % 5) as usize;
            for _ in 0..degree {
                *random = next_random(*random);
                let child = (*random % nodes as u64) as u32;
                if !successors[parent as usize].contains(&child) {
                    successors[parent as usize].push(child);
                }
            }
        }
        let terminals: Vec<_> = (0..nodes)
            .filter(|&node| successors[node as usize].is_empty())
            .collect();
        ExplicitGraph::new(successors, &terminals)
    }

    fn next_random(state: u64) -> u64 {
        state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407)
    }

    fn test_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tic-tac-chec-parallel-{}-{nonce}.ctb",
            std::process::id()
        ))
    }
}
