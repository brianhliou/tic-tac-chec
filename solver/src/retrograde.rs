//! Graph-safe win/loss/draw retrograde analysis for finite loopy games.
//!
//! The core stores one value byte and one remaining-child byte per node. It
//! never assumes the graph is acyclic: nodes left unresolved after the queue
//! reaches its fixpoint are draws.

use std::collections::VecDeque;
use std::fmt;

const UNKNOWN: u8 = 0;
const WIN: u8 = 1;
const LOSS: u8 = 2;
const DRAW: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Value {
    Win,
    Loss,
    Draw,
}

/// A finite directed game graph whose edges alternate the player to move.
pub trait GameGraph {
    fn node_count(&self) -> u32;

    fn is_terminal_loss(&self, node: u32) -> bool;

    fn for_each_successor(&self, node: u32, emit: impl FnMut(u32));

    fn for_each_predecessor(&self, node: u32, emit: impl FnMut(u32));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolutionStats {
    pub wins: u64,
    pub losses: u64,
    pub draws: u64,
}

pub struct Solution {
    values: Vec<u8>,
    stats: SolutionStats,
}

impl Solution {
    pub fn value(&self, node: u32) -> Value {
        decode(self.values[node as usize])
    }

    pub const fn stats(&self) -> SolutionStats {
        self.stats
    }

    pub fn audit(&self, graph: &impl GameGraph) -> Result<(), RetrogradeError> {
        let node_count = graph.node_count();
        if self.values.len() != node_count as usize {
            return Err(RetrogradeError::NodeCountMismatch {
                expected: self.values.len() as u64,
                actual: node_count as u64,
            });
        }

        for node in 0..node_count {
            let mut successor_count = 0_u16;
            let mut has_loss = false;
            let mut all_win = true;
            let mut bad_edge = None;
            graph.for_each_successor(node, |child| {
                if child >= node_count {
                    bad_edge = Some(child);
                    return;
                }
                successor_count += 1;
                match self.value(child) {
                    Value::Loss => has_loss = true,
                    Value::Win => {}
                    Value::Draw => all_win = false,
                }
            });
            if let Some(child) = bad_edge {
                return Err(RetrogradeError::EdgeOutOfRange {
                    from: node,
                    to: child,
                });
            }
            if graph.is_terminal_loss(node) && successor_count != 0 {
                return Err(RetrogradeError::TerminalHasSuccessors {
                    node,
                    count: successor_count,
                });
            }

            let expected = if graph.is_terminal_loss(node) || successor_count == 0 {
                Value::Loss
            } else if has_loss {
                Value::Win
            } else if all_win {
                Value::Loss
            } else {
                Value::Draw
            };
            let actual = self.value(node);
            if actual != expected {
                return Err(RetrogradeError::AuditMismatch {
                    node,
                    expected,
                    actual,
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrogradeError {
    EdgeOutOfRange {
        from: u32,
        to: u32,
    },
    TerminalHasSuccessors {
        node: u32,
        count: u16,
    },
    DegreeOverflow {
        node: u32,
        count: u16,
    },
    CounterUnderflow {
        node: u32,
    },
    AuditMismatch {
        node: u32,
        expected: Value,
        actual: Value,
    },
    NodeCountMismatch {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for RetrogradeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RetrogradeError {}

pub fn solve(graph: &impl GameGraph) -> Result<Solution, RetrogradeError> {
    let node_count = graph.node_count();
    let mut values = vec![UNKNOWN; node_count as usize];
    let mut remaining = vec![0_u8; node_count as usize];
    let mut queue = VecDeque::new();

    for node in 0..node_count {
        let mut successor_count = 0_u16;
        let mut bad_edge = None;
        graph.for_each_successor(node, |child| {
            if child >= node_count {
                bad_edge = Some(child);
            } else {
                successor_count += 1;
            }
        });
        if let Some(child) = bad_edge {
            return Err(RetrogradeError::EdgeOutOfRange {
                from: node,
                to: child,
            });
        }
        if successor_count > u8::MAX as u16 {
            return Err(RetrogradeError::DegreeOverflow {
                node,
                count: successor_count,
            });
        }
        if graph.is_terminal_loss(node) && successor_count != 0 {
            return Err(RetrogradeError::TerminalHasSuccessors {
                node,
                count: successor_count,
            });
        }

        remaining[node as usize] = successor_count as u8;
        if graph.is_terminal_loss(node) || successor_count == 0 {
            values[node as usize] = LOSS;
            queue.push_back(node);
        }
    }

    while let Some(child) = queue.pop_front() {
        let child_value = values[child as usize];
        let mut bad_parent = None;
        let mut counter_underflow = None;
        graph.for_each_predecessor(child, |parent| {
            if parent >= node_count {
                bad_parent = Some(parent);
                return;
            }
            if values[parent as usize] != UNKNOWN {
                return;
            }

            match child_value {
                LOSS => {
                    values[parent as usize] = WIN;
                    queue.push_back(parent);
                }
                WIN => {
                    let Some(next) = remaining[parent as usize].checked_sub(1) else {
                        counter_underflow = Some(parent);
                        return;
                    };
                    remaining[parent as usize] = next;
                    if next == 0 {
                        values[parent as usize] = LOSS;
                        queue.push_back(parent);
                    }
                }
                _ => unreachable!("only resolved wins and losses enter the queue"),
            }
        });
        if let Some(parent) = bad_parent {
            return Err(RetrogradeError::EdgeOutOfRange {
                from: parent,
                to: child,
            });
        }
        if let Some(node) = counter_underflow {
            return Err(RetrogradeError::CounterUnderflow { node });
        }
    }

    let mut stats = SolutionStats {
        wins: 0,
        losses: 0,
        draws: 0,
    };
    for value in &mut values {
        match *value {
            WIN => stats.wins += 1,
            LOSS => stats.losses += 1,
            UNKNOWN => {
                *value = DRAW;
                stats.draws += 1;
            }
            _ => unreachable!("draws are assigned only after the fixpoint"),
        }
    }

    Ok(Solution { values, stats })
}

fn decode(value: u8) -> Value {
    match value {
        WIN => Value::Win,
        LOSS => Value::Loss,
        DRAW => Value::Draw,
        _ => unreachable!("completed solutions contain no unresolved values"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    if let Some(parents) = predecessors.get_mut(child as usize) {
                        parents.push(parent as u32);
                    }
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
    fn forced_chain_alternates_losses_and_wins() {
        let graph = ExplicitGraph::new(vec![vec![1], vec![2], vec![]], &[2]);
        let solution = solve(&graph).unwrap();
        assert_eq!(
            solution_values(&solution, 3),
            [Value::Loss, Value::Win, Value::Loss]
        );
        assert_eq!(
            solution.stats(),
            SolutionStats {
                wins: 1,
                losses: 2,
                draws: 0
            }
        );
        solution.audit(&graph).unwrap();
    }

    #[test]
    fn closed_cycle_is_drawn() {
        let graph = ExplicitGraph::new(vec![vec![1], vec![0]], &[]);
        let solution = solve(&graph).unwrap();
        assert_eq!(solution_values(&solution, 2), [Value::Draw, Value::Draw]);
        solution.audit(&graph).unwrap();
    }

    #[test]
    fn terminal_escape_resolves_a_cycle() {
        let graph = ExplicitGraph::new(vec![vec![1, 2], vec![0], vec![]], &[2]);
        let solution = solve(&graph).unwrap();
        assert_eq!(
            solution_values(&solution, 3),
            [Value::Win, Value::Loss, Value::Loss]
        );
        solution.audit(&graph).unwrap();
    }

    #[test]
    fn draw_is_preferred_over_a_forced_loss() {
        let graph = ExplicitGraph::new(vec![vec![1, 3], vec![1], vec![], vec![2]], &[2]);
        let solution = solve(&graph).unwrap();
        assert_eq!(
            solution_values(&solution, 4),
            [Value::Draw, Value::Draw, Value::Loss, Value::Win]
        );
        solution.audit(&graph).unwrap();
    }

    #[test]
    fn queue_solver_matches_independent_pull_fixpoint() {
        let mut random = 0x3c6e_f372_fe94_f82b_u64;
        for nodes in 1..=40_u32 {
            for _ in 0..100 {
                let mut successors = vec![Vec::new(); nodes as usize];
                for parent in 0..nodes {
                    random = next_random(random);
                    let degree = (random % 5) as usize;
                    for _ in 0..degree {
                        random = next_random(random);
                        let child = (random % nodes as u64) as u32;
                        if !successors[parent as usize].contains(&child) {
                            successors[parent as usize].push(child);
                        }
                    }
                }
                let terminals: Vec<_> = (0..nodes)
                    .filter(|&node| successors[node as usize].is_empty())
                    .collect();
                let graph = ExplicitGraph::new(successors, &terminals);
                let solution = solve(&graph).unwrap();
                let expected = independent_pull_fixpoint(&graph);
                assert_eq!(solution_values(&solution, nodes), expected);
                solution.audit(&graph).unwrap();
            }
        }
    }

    fn independent_pull_fixpoint(graph: &ExplicitGraph) -> Vec<Value> {
        let mut values = vec![None; graph.node_count() as usize];
        for node in 0..graph.node_count() {
            if graph.is_terminal_loss(node) || graph.successors[node as usize].is_empty() {
                values[node as usize] = Some(Value::Loss);
            }
        }

        loop {
            let mut changed = false;
            for node in 0..graph.node_count() {
                if values[node as usize].is_some() {
                    continue;
                }
                let children = &graph.successors[node as usize];
                if children
                    .iter()
                    .any(|&child| values[child as usize] == Some(Value::Loss))
                {
                    values[node as usize] = Some(Value::Win);
                    changed = true;
                } else if children
                    .iter()
                    .all(|&child| values[child as usize] == Some(Value::Win))
                {
                    values[node as usize] = Some(Value::Loss);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        values
            .into_iter()
            .map(|value| value.unwrap_or(Value::Draw))
            .collect()
    }

    fn solution_values(solution: &Solution, nodes: u32) -> Vec<Value> {
        (0..nodes).map(|node| solution.value(node)).collect()
    }

    fn next_random(state: u64) -> u64 {
        state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407)
    }
}
