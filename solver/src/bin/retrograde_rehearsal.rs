use std::hint::black_box;
use std::mem::MaybeUninit;
use std::time::Instant;

use tic_tac_chec::graph::for_each_predecessor;
use tic_tac_chec::ranking::{PostOpeningId, POST_OPENING_DOMAIN};
use tic_tac_chec::retrograde::Value;
use tic_tac_chec::Rules;

const UNKNOWN: u8 = 0;
const WIN: u8 = 1;
const LOSS: u8 = 2;

fn main() {
    let child_iterations = argument(1, 100_000) as u64;
    let initial_remaining = argument(2, 12);
    assert!(
        (1..=u8::MAX as usize).contains(&initial_remaining),
        "initial counter must fit a positive byte"
    );
    let initial_remaining = initial_remaining as u8;

    let nodes = POST_OPENING_DOMAIN as usize;
    let mut rehearsal = Rehearsal::new(nodes, initial_remaining);
    prefault(&mut rehearsal.values);
    prefault(&mut rehearsal.remaining);
    let peak_rss_before_mib = peak_rss_mib();

    let mut random = 0x1f83_d9ab_fb41_bd6b_u64;
    let start = Instant::now();
    for iteration in 0..child_iterations {
        random = next_random(random);
        let child = PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap();
        let child_value = if iteration.is_multiple_of(2) {
            Value::Loss
        } else {
            Value::Win
        };
        for_each_predecessor(child, Rules::default(), |parent| {
            rehearsal.propagate(child_value, parent.get());
        });
    }
    let elapsed = start.elapsed();
    let peak_rss_after_mib = peak_rss_mib();

    println!("nodes = {nodes}");
    println!(
        "table_gib = {:.3}",
        nodes as f64 * 2.0 / (1024.0 * 1024.0 * 1024.0)
    );
    println!("child_iterations = {child_iterations}");
    println!("initial_remaining = {initial_remaining}");
    if let Some(rss) = peak_rss_before_mib {
        println!("peak_rss_before_mib = {rss:.1}");
    }
    if let Some(rss) = peak_rss_after_mib {
        println!("peak_rss_after_mib = {rss:.1}");
    }
    println!(
        "predecessor_edges = {}",
        rehearsal.metrics.predecessor_edges
    );
    println!("skipped_resolved = {}", rehearsal.metrics.skipped_resolved);
    println!(
        "counter_decrements = {}",
        rehearsal.metrics.counter_decrements
    );
    println!("resolved_wins = {}", rehearsal.metrics.resolved_wins);
    println!("resolved_losses = {}", rehearsal.metrics.resolved_losses);
    println!("frontier_len = {}", rehearsal.frontier.len());
    println!(
        "frontier_mib = {:.3}",
        rehearsal.frontier.capacity() as f64 * 4.0 / (1024.0 * 1024.0)
    );
    println!("seconds = {:.6}", elapsed.as_secs_f64());
    println!(
        "ns_per_child = {:.2}",
        elapsed.as_nanos() as f64 / child_iterations as f64
    );
    println!(
        "ns_per_predecessor = {:.2}",
        elapsed.as_nanos() as f64 / rehearsal.metrics.predecessor_edges as f64
    );
    println!(
        "millions_of_predecessors_per_second = {:.3}",
        rehearsal.metrics.predecessor_edges as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("checksum = {}", rehearsal.metrics.checksum);
    black_box(rehearsal);
}

struct Rehearsal {
    values: Vec<u8>,
    remaining: Vec<u8>,
    frontier: Vec<u32>,
    metrics: Metrics,
}

impl Rehearsal {
    fn new(nodes: usize, initial_remaining: u8) -> Self {
        Self {
            values: vec![UNKNOWN; nodes],
            remaining: vec![initial_remaining; nodes],
            frontier: Vec::new(),
            metrics: Metrics::default(),
        }
    }

    fn propagate(&mut self, child_value: Value, parent: u32) {
        self.metrics.predecessor_edges += 1;
        let index = parent as usize;
        if self.values[index] != UNKNOWN {
            self.metrics.skipped_resolved += 1;
            return;
        }

        match child_value {
            Value::Loss => {
                self.values[index] = WIN;
                self.frontier.push(parent);
                self.metrics.resolved_wins += 1;
            }
            Value::Win => {
                self.remaining[index] = self.remaining[index]
                    .checked_sub(1)
                    .expect("unresolved counter cannot underflow");
                self.metrics.counter_decrements += 1;
                if self.remaining[index] == 0 {
                    self.values[index] = LOSS;
                    self.frontier.push(parent);
                    self.metrics.resolved_losses += 1;
                }
            }
            Value::Draw => unreachable!("draws never enter the retrograde frontier"),
        }
        self.metrics.checksum = self
            .metrics
            .checksum
            .wrapping_add(parent as u64 + self.values[index] as u64 + self.remaining[index] as u64);
    }
}

#[derive(Default)]
struct Metrics {
    predecessor_edges: u64,
    skipped_resolved: u64,
    counter_decrements: u64,
    resolved_wins: u64,
    resolved_losses: u64,
    checksum: u64,
}

fn prefault(table: &mut [u8]) {
    for page in table.chunks_mut(4096) {
        let value = black_box(page[0]);
        page[0] = value;
        black_box(page[0]);
    }
}

fn argument(index: usize, default: usize) -> usize {
    std::env::args()
        .nth(index)
        .map(|value| value.parse().expect("arguments must be positive integers"))
        .unwrap_or(default)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn peak_rss_mib() -> Option<f64> {
    #[repr(C)]
    struct Rusage {
        words: [std::ffi::c_long; 18],
    }

    unsafe extern "C" {
        fn getrusage(who: std::ffi::c_int, usage: *mut Rusage) -> std::ffi::c_int;
    }

    let mut usage = MaybeUninit::<Rusage>::uninit();
    // RUSAGE_SELF is zero on Darwin and Linux. Both 64-bit ABIs lay out two
    // timevals followed by fourteen longs; ru_maxrss is the fifth long word.
    if unsafe { getrusage(0, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    let max_rss = unsafe { usage.assume_init() }.words[4] as f64;
    #[cfg(target_os = "macos")]
    return Some(max_rss / (1024.0 * 1024.0));
    #[cfg(target_os = "linux")]
    return Some(max_rss / 1024.0);
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peak_rss_mib() -> Option<f64> {
    None
}

fn next_random(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loss_child_resolves_parent_win_once() {
        let mut rehearsal = Rehearsal::new(4, 2);
        rehearsal.propagate(Value::Loss, 1);
        rehearsal.propagate(Value::Loss, 1);
        assert_eq!(rehearsal.values, [UNKNOWN, WIN, UNKNOWN, UNKNOWN]);
        assert_eq!(rehearsal.frontier, [1]);
        assert_eq!(rehearsal.metrics.resolved_wins, 1);
        assert_eq!(rehearsal.metrics.skipped_resolved, 1);
    }

    #[test]
    fn all_win_children_resolve_parent_loss() {
        let mut rehearsal = Rehearsal::new(4, 2);
        rehearsal.propagate(Value::Win, 2);
        rehearsal.propagate(Value::Win, 2);
        rehearsal.propagate(Value::Win, 2);
        assert_eq!(rehearsal.values, [UNKNOWN, UNKNOWN, LOSS, UNKNOWN]);
        assert_eq!(rehearsal.remaining[2], 0);
        assert_eq!(rehearsal.frontier, [2]);
        assert_eq!(rehearsal.metrics.counter_decrements, 2);
        assert_eq!(rehearsal.metrics.resolved_losses, 1);
        assert_eq!(rehearsal.metrics.skipped_resolved, 1);
    }
}
