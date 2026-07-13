use std::hint::black_box;
use std::time::Instant;

fn main() {
    let mebibytes_per_table = argument(1, 64);
    let iterations = argument(2, 10_000_000) as u64;
    assert!(
        mebibytes_per_table.is_power_of_two(),
        "MiB per table must be a power of two"
    );

    let nodes = mebibytes_per_table
        .checked_mul(1024 * 1024)
        .expect("table size must fit usize");
    let mut values = vec![0_u8; nodes];
    let mut remaining = vec![0_u8; nodes];

    // Force physical page allocation before the timed random-access section.
    values.fill(1);
    remaining.fill(32);

    let mask = nodes - 1;
    let mut random = 0x510e_527f_ade6_82d1_u64;
    let mut checksum = 0_u64;
    let start = Instant::now();
    for _ in 0..iterations {
        random = next_random(random);
        let index = black_box(random as usize & mask);
        let value = values[index];
        let counter = remaining[index];
        values[index] = value.rotate_left(1) ^ counter;
        remaining[index] = counter.wrapping_sub(1);
        checksum = checksum.wrapping_add(value as u64 + counter as u64);
    }
    let elapsed = start.elapsed();

    println!("mebibytes_per_table = {mebibytes_per_table}");
    println!("total_mebibytes = {}", mebibytes_per_table * 2);
    println!("nodes = {nodes}");
    println!("iterations = {iterations}");
    println!("seconds = {:.6}", elapsed.as_secs_f64());
    println!(
        "ns_per_paired_update = {:.2}",
        elapsed.as_nanos() as f64 / iterations as f64
    );
    println!(
        "millions_of_updates_per_second = {:.3}",
        iterations as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("checksum = {checksum}");
    black_box((&values, &remaining));
}

fn argument(index: usize, default: usize) -> usize {
    std::env::args()
        .nth(index)
        .map(|value| value.parse().expect("arguments must be positive integers"))
        .unwrap_or(default)
}

fn next_random(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}
