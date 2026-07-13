use std::hint::black_box;
use std::time::Instant;

use tic_tac_chec::graph::for_each_successor;
use tic_tac_chec::ranking::{PostOpeningId, POST_OPENING_DOMAIN};
use tic_tac_chec::Rules;

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|value| value.parse::<u64>().expect("iterations must be an integer"))
        .unwrap_or(1_000_000);

    let mut random = 0x082e_fa98_ec4e_6c89_u64;
    let mut ids = Vec::with_capacity(65_536);
    for _ in 0..65_536 {
        random = next_random(random);
        ids.push(PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap());
    }

    let mut edges = 0_u64;
    let mut zero_successor_states = 0_u64;
    let mut checksum = 0_u64;
    let start = Instant::now();
    for index in 0..iterations {
        let id = black_box(ids[index as usize % ids.len()]);
        let count = for_each_successor(id, Rules::default(), |child| {
            checksum = checksum.wrapping_add(black_box(child.get()) as u64);
        });
        edges += count as u64;
        zero_successor_states += u64::from(count == 0);
    }
    let elapsed = start.elapsed();

    println!("iterations = {iterations}");
    println!("edges = {edges}");
    println!("zero_successor_states = {zero_successor_states}");
    println!(
        "mean_edges_per_state = {:.3}",
        edges as f64 / iterations as f64
    );
    println!("seconds = {:.6}", elapsed.as_secs_f64());
    println!(
        "ns_per_state = {:.2}",
        elapsed.as_nanos() as f64 / iterations as f64
    );
    println!(
        "ns_per_edge = {:.2}",
        elapsed.as_nanos() as f64 / edges as f64
    );
    println!(
        "millions_of_edges_per_second = {:.3}",
        edges as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("checksum = {checksum}");
}

fn next_random(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}
