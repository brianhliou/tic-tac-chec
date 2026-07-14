use std::hint::black_box;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use tic_tac_chec::graph::for_each_predecessor;
use tic_tac_chec::ranking::{PostOpeningId, POST_OPENING_DOMAIN};
use tic_tac_chec::retrograde::Value;
use tic_tac_chec::Rules;

const UNKNOWN: u8 = 0;
const WIN: u8 = 1;
const LOSS: u8 = 2;

fn main() {
    let child_iterations = argument(1, 1_000_000) as u64;
    let threads = argument(2, 4);
    let initial_remaining = argument(3, 12);
    assert!(threads > 0, "thread count must be positive");
    assert!(
        (1..=u8::MAX as usize).contains(&initial_remaining),
        "initial counter must fit a positive byte"
    );
    let initial_remaining = initial_remaining as u8;

    let nodes = POST_OPENING_DOMAIN as usize;
    let mut value_bytes = vec![UNKNOWN; nodes];
    let mut remaining_bytes = vec![initial_remaining; nodes];
    prefault(&mut value_bytes);
    prefault(&mut remaining_bytes);
    let peak_rss_before_mib = peak_rss_mib();

    let values = as_atomic_bytes(&value_bytes);
    let remaining = as_atomic_bytes(&remaining_bytes);
    let start = Instant::now();
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for thread in 0..threads {
            let first = child_iterations * thread as u64 / threads as u64;
            let end = child_iterations * (thread + 1) as u64 / threads as u64;
            handles.push(scope.spawn(move || run_thread(thread, first, end, values, remaining)));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("rehearsal thread must not panic"))
            .collect::<Vec<_>>()
    });
    let elapsed = start.elapsed();
    let peak_rss_after_mib = peak_rss_mib();

    let mut totals = ThreadMetrics::default();
    let mut frontier_capacity = 0_usize;
    for result in &results {
        totals.add(&result.metrics);
        frontier_capacity += result.frontier.capacity();
    }

    println!("nodes = {nodes}");
    println!(
        "table_gib = {:.3}",
        nodes as f64 * 2.0 / (1024.0 * 1024.0 * 1024.0)
    );
    println!("child_iterations = {child_iterations}");
    println!("threads = {threads}");
    println!("initial_remaining = {initial_remaining}");
    if let Some(rss) = peak_rss_before_mib {
        println!("peak_rss_before_mib = {rss:.1}");
    }
    if let Some(rss) = peak_rss_after_mib {
        println!("peak_rss_after_mib = {rss:.1}");
    }
    println!("predecessor_edges = {}", totals.predecessor_edges);
    println!("skipped_resolved = {}", totals.skipped_resolved);
    println!("counter_decrements = {}", totals.counter_decrements);
    println!("resolved_wins = {}", totals.resolved_wins);
    println!("resolved_losses = {}", totals.resolved_losses);
    println!("counter_underflows = {}", totals.counter_underflows);
    println!(
        "frontier_len = {}",
        totals.resolved_wins + totals.resolved_losses
    );
    println!(
        "frontier_mib = {:.3}",
        frontier_capacity as f64 * 4.0 / (1024.0 * 1024.0)
    );
    println!("seconds = {:.6}", elapsed.as_secs_f64());
    println!(
        "ns_per_child = {:.2}",
        elapsed.as_nanos() as f64 / child_iterations as f64
    );
    println!(
        "ns_per_predecessor = {:.2}",
        elapsed.as_nanos() as f64 / totals.predecessor_edges as f64
    );
    println!(
        "millions_of_predecessors_per_second = {:.3}",
        totals.predecessor_edges as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("checksum = {}", totals.checksum);
    black_box((results, value_bytes, remaining_bytes));
}

fn run_thread(
    thread: usize,
    first: u64,
    end: u64,
    values: &[AtomicU8],
    remaining: &[AtomicU8],
) -> ThreadResult {
    let mut random =
        0x5be0_cd19_137e_2179_u64 ^ (thread as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mut result = ThreadResult::default();
    for local_iteration in 0..end - first {
        random = next_random(random);
        let child = PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap();
        let child_value = if (first + local_iteration).is_multiple_of(2) {
            Value::Loss
        } else {
            Value::Win
        };
        for_each_predecessor(child, Rules::default(), |parent| {
            propagate_atomic(child_value, parent.get(), values, remaining, &mut result);
        });
    }
    result
}

fn propagate_atomic(
    child_value: Value,
    parent: u32,
    values: &[AtomicU8],
    remaining: &[AtomicU8],
    result: &mut ThreadResult,
) {
    result.metrics.predecessor_edges += 1;
    let index = parent as usize;

    match child_value {
        Value::Loss => {
            if values[index]
                .compare_exchange(UNKNOWN, WIN, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                result.frontier.push(parent);
                result.metrics.resolved_wins += 1;
                result.metrics.checksum = result.metrics.checksum.wrapping_add(
                    parent as u64 + WIN as u64 + remaining[index].load(Ordering::Relaxed) as u64,
                );
            } else {
                result.metrics.skipped_resolved += 1;
            }
        }
        Value::Win => {
            if values[index].load(Ordering::Relaxed) != UNKNOWN {
                result.metrics.skipped_resolved += 1;
                return;
            }
            let previous = remaining[index].fetch_sub(1, Ordering::Relaxed);
            if previous == 0 {
                remaining[index].fetch_add(1, Ordering::Relaxed);
                result.metrics.counter_underflows += 1;
                return;
            }
            result.metrics.counter_decrements += 1;
            if previous == 1 {
                if values[index]
                    .compare_exchange(UNKNOWN, LOSS, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    result.frontier.push(parent);
                    result.metrics.resolved_losses += 1;
                    result.metrics.checksum = result.metrics.checksum.wrapping_add(
                        parent as u64
                            + LOSS as u64
                            + remaining[index].load(Ordering::Relaxed) as u64,
                    );
                } else {
                    result.metrics.skipped_resolved += 1;
                }
            } else {
                result.metrics.checksum = result
                    .metrics
                    .checksum
                    .wrapping_add(parent as u64 + UNKNOWN as u64 + previous as u64 - 1);
            }
        }
        Value::Draw => unreachable!("draws never enter the retrograde frontier"),
    }
}

#[derive(Default)]
struct ThreadResult {
    metrics: ThreadMetrics,
    frontier: Vec<u32>,
}

#[derive(Default)]
struct ThreadMetrics {
    predecessor_edges: u64,
    skipped_resolved: u64,
    counter_decrements: u64,
    resolved_wins: u64,
    resolved_losses: u64,
    counter_underflows: u64,
    checksum: u64,
}

impl ThreadMetrics {
    fn add(&mut self, other: &Self) {
        self.predecessor_edges += other.predecessor_edges;
        self.skipped_resolved += other.skipped_resolved;
        self.counter_decrements += other.counter_decrements;
        self.resolved_wins += other.resolved_wins;
        self.resolved_losses += other.resolved_losses;
        self.counter_underflows += other.counter_underflows;
        self.checksum = self.checksum.wrapping_add(other.checksum);
    }
}

fn as_atomic_bytes(bytes: &[u8]) -> &[AtomicU8] {
    assert_eq!(std::mem::size_of::<AtomicU8>(), std::mem::size_of::<u8>());
    assert_eq!(std::mem::align_of::<AtomicU8>(), std::mem::align_of::<u8>());
    // SAFETY: `AtomicU8` has the asserted byte layout. After conversion, the
    // backing vectors are accessed only through these atomic references until
    // every scoped worker has joined and the references are no longer used.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<AtomicU8>(), bytes.len()) }
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
    fn atomic_updates_follow_retrograde_transitions() {
        let values = [AtomicU8::new(UNKNOWN), AtomicU8::new(UNKNOWN)];
        let remaining = [AtomicU8::new(2), AtomicU8::new(2)];
        let mut result = ThreadResult::default();

        propagate_atomic(Value::Loss, 0, &values, &remaining, &mut result);
        propagate_atomic(Value::Loss, 0, &values, &remaining, &mut result);
        propagate_atomic(Value::Win, 1, &values, &remaining, &mut result);
        propagate_atomic(Value::Win, 1, &values, &remaining, &mut result);
        propagate_atomic(Value::Win, 1, &values, &remaining, &mut result);

        assert_eq!(values[0].load(Ordering::Relaxed), WIN);
        assert_eq!(values[1].load(Ordering::Relaxed), LOSS);
        assert_eq!(remaining[1].load(Ordering::Relaxed), 0);
        assert_eq!(result.frontier, [0, 1]);
        assert_eq!(result.metrics.resolved_wins, 1);
        assert_eq!(result.metrics.resolved_losses, 1);
        assert_eq!(result.metrics.skipped_resolved, 2);
        assert_eq!(result.metrics.counter_underflows, 0);
    }

    #[test]
    fn concurrent_final_decrement_resolves_loss_once() {
        let values = [AtomicU8::new(UNKNOWN)];
        let remaining = [AtomicU8::new(64)];
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..8 {
                handles.push(scope.spawn(|| {
                    let mut result = ThreadResult::default();
                    for _ in 0..8 {
                        propagate_atomic(Value::Win, 0, &values, &remaining, &mut result);
                    }
                    result
                }));
            }
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });

        assert_eq!(values[0].load(Ordering::Relaxed), LOSS);
        assert_eq!(remaining[0].load(Ordering::Relaxed), 0);
        assert_eq!(
            results
                .iter()
                .map(|result| result.metrics.resolved_losses)
                .sum::<u64>(),
            1
        );
        assert_eq!(
            results
                .iter()
                .map(|result| result.metrics.counter_underflows)
                .sum::<u64>(),
            0
        );
    }

    #[test]
    fn concurrent_loss_witness_overrides_win_child_decrements() {
        let values = [AtomicU8::new(UNKNOWN)];
        let remaining = [AtomicU8::new(65)];
        std::thread::scope(|scope| {
            for thread in 0..9 {
                let values = &values;
                let remaining = &remaining;
                scope.spawn(move || {
                    let mut result = ThreadResult::default();
                    if thread == 8 {
                        propagate_atomic(Value::Loss, 0, values, remaining, &mut result);
                    } else {
                        for _ in 0..8 {
                            propagate_atomic(Value::Win, 0, values, remaining, &mut result);
                        }
                    }
                });
            }
        });

        assert_eq!(values[0].load(Ordering::Relaxed), WIN);
        assert!(remaining[0].load(Ordering::Relaxed) >= 1);
    }
}
