use std::hint::black_box;
use std::time::Instant;

use tic_tac_chec::ranking::{
    rank_post_opening, swap_sides_and_rotate, unrank_post_opening, PostOpeningId,
    POST_OPENING_DOMAIN,
};

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|value| value.parse::<u64>().expect("iterations must be an integer"))
        .unwrap_or(5_000_000);

    let mut random = 0x243f_6a88_85a3_08d3_u64;
    let mut ids = Vec::with_capacity(65_536);
    for _ in 0..65_536 {
        random = next_random(random);
        ids.push(PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap());
    }
    let positions: Vec<_> = ids.iter().copied().map(unrank_post_opening).collect();
    let black_positions: Vec<_> = positions.iter().map(swap_sides_and_rotate).collect();

    let start = Instant::now();
    for index in 0..iterations {
        black_box(unrank_post_opening(black_box(
            ids[index as usize % ids.len()],
        )));
    }
    report("unrank", iterations, start.elapsed());

    let mut checksum = 0_u64;
    let start = Instant::now();
    for index in 0..iterations {
        let position = black_box(&positions[index as usize % positions.len()]);
        checksum =
            checksum.wrapping_add(black_box(rank_post_opening(position).unwrap().get()) as u64);
    }
    report("rank_normalized", iterations, start.elapsed());
    println!("rank_normalized_checksum = {checksum}");

    let mut checksum = 0_u64;
    let start = Instant::now();
    for index in 0..iterations {
        let position = black_box(&black_positions[index as usize % black_positions.len()]);
        checksum =
            checksum.wrapping_add(black_box(rank_post_opening(position).unwrap().get()) as u64);
    }
    report("rank_black_to_move", iterations, start.elapsed());
    println!("rank_black_to_move_checksum = {checksum}");

    random = 0x243f_6a88_85a3_08d3_u64;
    let mut checksum = 0_u64;
    let start = Instant::now();
    for _ in 0..iterations {
        random = next_random(random);
        let raw = (random % POST_OPENING_DOMAIN as u64) as u32;
        let id = PostOpeningId::new(raw).unwrap();
        let position = black_box(unrank_post_opening(black_box(id)));
        checksum =
            checksum.wrapping_add(black_box(rank_post_opening(&position).unwrap().get()) as u64);
    }
    let elapsed = start.elapsed();
    println!("iterations = {iterations}");
    report("rank_unrank", iterations, elapsed);
    println!("checksum = {checksum}");
}

fn next_random(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

fn report(name: &str, iterations: u64, elapsed: std::time::Duration) {
    let nanos = elapsed.as_nanos() as f64 / iterations as f64;
    println!("{name}_seconds = {:.6}", elapsed.as_secs_f64());
    println!("{name}_ns = {nanos:.2}");
    println!("{name}_millions_per_second = {:.3}", 1_000.0 / nanos);
}
