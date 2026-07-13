fn main() {
    let placements = (0..=8)
        .map(|pieces| choose(8, pieces) * permutations(16, pieces))
        .sum::<u64>();

    let oriented_arrangements = (0..=6)
        .map(|non_pawns| {
            let subsets = choose(6, non_pawns);
            subsets
                * (permutations(16, non_pawns)
                    + 2 * 24 * permutations(15, non_pawns)
                    + 536 * permutations(14, non_pawns))
        })
        .sum::<u64>();

    let opening_by_ply: Vec<u64> = (0_u64..=5)
        .map(|ply| {
            let white = ply.div_ceil(2);
            let black = ply / 2;
            choose(4, white) * choose(4, black) * permutations(16, white + black)
        })
        .collect();
    let locked_opening = opening_by_ply.iter().sum::<u64>();
    let dense_states = 2 * oriented_arrangements + locked_opening;

    println!("board/hand arrangements:       {placements:>15}");
    println!("direction-aware arrangements:  {oriented_arrangements:>15}");
    println!(
        "post-opening states (+ turn):   {:>15}",
        2 * oriented_arrangements
    );
    for (ply, count) in opening_by_ply.iter().enumerate() {
        println!("locked opening ply {ply}:         {count:>15}");
    }
    println!("locked opening states:          {locked_opening:>15}");
    println!("dense all-legal state domain:   {dense_states:>15}");
    println!();
    println!(
        "2-bit values:                   {:>8.2} GiB",
        gib(dense_states, 0.25)
    );
    println!(
        "u8 child counts:                {:>8.2} GiB",
        gib(dense_states, 1.0)
    );
    println!(
        "u16 distances:                  {:>8.2} GiB",
        gib(dense_states, 2.0)
    );
    println!(
        "combined (3.25 bytes/state):    {:>8.2} GiB",
        gib(dense_states, 3.25)
    );
}

const fn choose(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }
    let k = if k < n - k { k } else { n - k };
    let mut result = 1;
    let mut i = 0;
    while i < k {
        result = result * (n - i) / (i + 1);
        i += 1;
    }
    result
}

const fn permutations(n: u64, k: u64) -> u64 {
    let mut result = 1;
    let mut i = 0;
    while i < k {
        result *= n - i;
        i += 1;
    }
    result
}

fn gib(states: u64, bytes_per_state: f64) -> f64 {
    states as f64 * bytes_per_state / 1024_f64.powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinatorics_are_stable() {
        assert_eq!(choose(8, 4), 70);
        assert_eq!(permutations(16, 8), 518_918_400);
    }
}
