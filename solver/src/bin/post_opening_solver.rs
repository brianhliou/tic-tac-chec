use std::error::Error;
use std::path::Path;
use std::time::Instant;

use tic_tac_chec::graph::PostOpeningGraph;
use tic_tac_chec::parallel::ParallelState;
use tic_tac_chec::Rules;

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(command) = arguments.get(1).map(String::as_str) else {
        usage();
    };
    let Some(path) = arguments.get(2) else {
        usage();
    };
    let threads = argument(&arguments, 3, 16);
    let rules = rules(&arguments);
    let graph = PostOpeningGraph::new(rules);

    match command {
        "init" => initialize(&graph, rules, path, threads),
        "propagate" => {
            let checkpoint_every = argument(&arguments, 4, 5) as u64;
            if checkpoint_every == 0 {
                return Err("checkpoint interval must be positive".into());
            }
            propagate(&graph, rules, path, threads, checkpoint_every)
        }
        _ => usage(),
    }
}

fn initialize(
    graph: &PostOpeningGraph,
    rules: Rules,
    path: impl AsRef<Path>,
    threads: usize,
) -> Result<(), Box<dyn Error>> {
    println!("phase = initialize");
    println!("threads = {threads}");
    println!("rules_tag = {:#010x}", rules.stable_tag());
    let start = Instant::now();
    let (state, stats) = ParallelState::initialize(graph, rules.stable_tag(), threads)?;
    let elapsed = start.elapsed();
    println!("nodes = {}", stats.nodes);
    println!("edges = {}", stats.edges);
    println!("terminal_losses = {}", stats.terminal_losses);
    println!("dead_end_losses = {}", stats.dead_end_losses);
    println!("maximum_degree = {}", stats.maximum_degree);
    println!("frontier = {}", stats.frontier);
    println!("initialization_seconds = {:.6}", elapsed.as_secs_f64());
    println!(
        "millions_of_nodes_per_second = {:.3}",
        stats.nodes as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!(
        "millions_of_edges_per_second = {:.3}",
        stats.edges as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );

    let checkpoint_start = Instant::now();
    state.save(path)?;
    println!(
        "checkpoint_seconds = {:.6}",
        checkpoint_start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn propagate(
    graph: &PostOpeningGraph,
    rules: Rules,
    path: impl AsRef<Path>,
    threads: usize,
    checkpoint_every: u64,
) -> Result<(), Box<dyn Error>> {
    println!("phase = propagate");
    println!("threads = {threads}");
    println!("checkpoint_every = {checkpoint_every}");
    let mut state = ParallelState::load(&path, graph, rules.stable_tag())?;
    println!("resume_wave = {}", state.wave());
    println!("resume_frontier = {}", state.frontier().len());

    while !state.frontier().is_empty() {
        let start = Instant::now();
        let stats = state.run_wave(graph, threads)?;
        let elapsed = start.elapsed();
        println!(
            "wave={} input={} edges={} skipped={} decrements={} wins={} losses={} output={} seconds={:.6} million_edges_per_second={:.3}",
            stats.wave,
            stats.input_frontier,
            stats.predecessor_edges,
            stats.skipped_resolved,
            stats.counter_decrements,
            stats.resolved_wins,
            stats.resolved_losses,
            stats.output_frontier,
            elapsed.as_secs_f64(),
            stats.predecessor_edges as f64 / elapsed.as_secs_f64() / 1_000_000.0,
        );

        if state.wave().is_multiple_of(checkpoint_every) || state.frontier().is_empty() {
            let checkpoint_start = Instant::now();
            state.save(&path)?;
            println!(
                "checkpoint_wave={} checkpoint_seconds={:.6}",
                state.wave(),
                checkpoint_start.elapsed().as_secs_f64()
            );
        }
    }

    let solution = state.finish()?;
    let stats = solution.stats();
    println!("fixpoint_wins = {}", stats.wins);
    println!("fixpoint_losses = {}", stats.losses);
    println!("fixpoint_draws = {}", stats.draws);
    Ok(())
}

fn rules(arguments: &[String]) -> Rules {
    match arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--pawn="))
    {
        None | Some("travel") => Rules::ORIGINAL_TRAVEL_DIRECTION,
        Some("outbound") => Rules::ORIGINAL_OUTBOUND_ONLY,
        Some("opponent") => Rules::ORIGINAL_TOWARD_OPPONENT,
        Some(other) => panic!("unknown pawn variant: {other}"),
    }
}

fn argument(arguments: &[String], index: usize, default: usize) -> usize {
    arguments
        .get(index)
        .filter(|argument| !argument.starts_with("--"))
        .map(|argument| {
            argument
                .parse()
                .expect("numeric arguments must be integers")
        })
        .unwrap_or(default)
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  post_opening_solver init <checkpoint.ctb> [threads] [--pawn=travel|outbound|opponent]\n  post_opening_solver propagate <checkpoint.ctb> [threads] [checkpoint-every-waves] [--pawn=travel|outbound|opponent]"
    );
    std::process::exit(2)
}
