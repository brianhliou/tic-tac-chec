use std::error::Error;
use std::path::Path;
use std::time::Instant;

use tic_tac_chec::graph::PostOpeningGraph;
use tic_tac_chec::opening::{audit_opening, solve_opening};
use tic_tac_chec::parallel::{audit_parallel, ParallelState};
use tic_tac_chec::ranking::{LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN};
use tic_tac_chec::remoteness::{
    audit_opening_parallel as audit_opening_remoteness, audit_parallel as audit_remoteness,
    solve_opening_parallel as solve_opening_remoteness, solve_parallel as solve_remoteness,
};
use tic_tac_chec::tablebase;
use tic_tac_chec::Rules;

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(command) = arguments.get(1).map(String::as_str) else {
        usage();
    };
    let Some(path) = arguments.get(2) else {
        usage();
    };
    let rules = rules(&arguments);
    let graph = PostOpeningGraph::new(rules);

    match command {
        "init" => initialize(&graph, rules, path, argument(&arguments, 3, 16)),
        "verify" => verify(&graph, rules, path),
        "audit" => audit(&graph, rules, path, argument(&arguments, 3, 16)),
        "opening" => opening(&graph, rules, path, argument(&arguments, 3, 16)),
        "enrich" => {
            let Some(output) = arguments
                .get(3)
                .filter(|argument| !argument.starts_with("--"))
            else {
                usage();
            };
            enrich(&graph, rules, path, output, argument(&arguments, 4, 16))
        }
        "propagate" => {
            let threads = argument(&arguments, 3, 16);
            let checkpoint_every = argument(&arguments, 4, 5) as u64;
            if checkpoint_every == 0 {
                return Err("checkpoint interval must be positive".into());
            }
            propagate(&graph, rules, path, threads, checkpoint_every)
        }
        _ => usage(),
    }
}

fn enrich(
    graph: &PostOpeningGraph,
    rules: Rules,
    checkpoint_path: impl AsRef<Path>,
    tablebase_path: impl AsRef<Path>,
    threads: usize,
) -> Result<(), Box<dyn Error>> {
    println!("phase = enrich");
    println!("threads = {threads}");
    println!("rules_tag = {:#010x}", rules.stable_tag());
    let load_start = Instant::now();
    let state = ParallelState::load(checkpoint_path, graph, rules.stable_tag())?;
    if !state.frontier().is_empty() {
        return Err("post-opening fixpoint must be complete".into());
    }
    let solution = state.finish()?;
    println!(
        "checkpoint_load_seconds = {:.6}",
        load_start.elapsed().as_secs_f64()
    );

    let solve_start = Instant::now();
    let (post, waves) = solve_remoteness(&solution, graph, threads)?;
    let solve_elapsed = solve_start.elapsed();
    let stats = post.stats();
    println!("nodes = {}", stats.nodes);
    println!("wins = {}", stats.wins);
    println!("losses = {}", stats.losses);
    println!("draws = {}", stats.draws);
    println!("terminal_losses = {}", stats.terminal_losses);
    println!("dead_end_losses = {}", stats.dead_end_losses);
    println!("initialized_loss_edges = {}", stats.initialized_loss_edges);
    println!("maximum_post_distance = {}", stats.maximum_distance);
    for wave in &waves {
        println!(
            "distance={} input={} predecessor_edges={} resolved={}",
            wave.distance, wave.input_frontier, wave.predecessor_edges, wave.resolved
        );
    }
    println!(
        "remoteness_solve_seconds = {:.6}",
        solve_elapsed.as_secs_f64()
    );

    let audit_start = Instant::now();
    let audited = audit_remoteness(&post, &solution, graph, threads)?;
    println!("remoteness_audit_nodes = {}", audited.nodes);
    println!(
        "remoteness_audit_decisive_nodes = {}",
        audited.decisive_nodes
    );
    println!(
        "remoteness_audit_decisive_edges = {}",
        audited.decisive_edges
    );
    println!(
        "remoteness_audit_maximum_distance = {}",
        audited.maximum_distance
    );
    println!(
        "remoteness_audit_seconds = {:.6}",
        audit_start.elapsed().as_secs_f64()
    );

    let opening_start = Instant::now();
    let opening = solve_opening(&post, rules, threads)?;
    let opening_distances = solve_opening_remoteness(&opening, &post, rules, threads)?;
    for layer in opening_distances.layers().iter().rev() {
        println!(
            "opening_ply={} states={} edges={} maximum_distance={}",
            layer.ply, layer.states, layer.edges, layer.maximum_distance
        );
    }
    println!(
        "opening_remoteness_seconds = {:.6}",
        opening_start.elapsed().as_secs_f64()
    );

    let opening_audit_start = Instant::now();
    let audited_opening =
        audit_opening_remoteness(&opening_distances, &opening, &post, rules, threads)?;
    if audited_opening != *opening_distances.layers() {
        return Err("opening remoteness audit layer counts differ from solve".into());
    }
    println!(
        "opening_remoteness_audit_seconds = {:.6}",
        opening_audit_start.elapsed().as_secs_f64()
    );

    let save_start = Instant::now();
    let checksum = tablebase::save_atomic(
        tablebase_path,
        rules.stable_tag(),
        post.codes(),
        opening_distances.codes(),
    )?;
    println!("tablebase_post_nodes = {POST_OPENING_DOMAIN}");
    println!("tablebase_opening_nodes = {LOCKED_OPENING_DOMAIN}");
    println!("tablebase_crc64 = {checksum:#018x}");
    println!(
        "tablebase_save_seconds = {:.6}",
        save_start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn opening(
    graph: &PostOpeningGraph,
    rules: Rules,
    path: impl AsRef<Path>,
    threads: usize,
) -> Result<(), Box<dyn Error>> {
    println!("phase = opening");
    println!("threads = {threads}");
    let state = ParallelState::load(path, graph, rules.stable_tag())?;
    if !state.frontier().is_empty() {
        return Err("post-opening fixpoint must be complete".into());
    }
    let post = state.finish()?;
    let start = Instant::now();
    let solution = solve_opening(&post, rules, threads)?;
    let solve_elapsed = start.elapsed();
    for layer in solution.layers().iter().rev() {
        println!(
            "ply={} states={} edges={} wins={} losses={} draws={}",
            layer.ply, layer.states, layer.edges, layer.wins, layer.losses, layer.draws
        );
    }
    println!("initial_value = {:?}", solution.initial_value());
    println!("opening_solve_seconds = {:.6}", solve_elapsed.as_secs_f64());

    let audit_start = Instant::now();
    let audited = audit_opening(&solution, &post, rules, threads)?;
    if audited != *solution.layers() {
        return Err("opening audit layer counts differ from solve".into());
    }
    println!(
        "opening_audit_seconds = {:.6}",
        audit_start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn audit(
    graph: &PostOpeningGraph,
    rules: Rules,
    path: impl AsRef<Path>,
    threads: usize,
) -> Result<(), Box<dyn Error>> {
    println!("phase = audit");
    println!("threads = {threads}");
    let state = ParallelState::load(path, graph, rules.stable_tag())?;
    if !state.frontier().is_empty() {
        return Err(format!(
            "fixpoint not reached: wave {} still has {} frontier states",
            state.wave(),
            state.frontier().len()
        )
        .into());
    }
    let solution = state.finish()?;
    let start = Instant::now();
    let stats = audit_parallel(&solution, graph, threads)?;
    let elapsed = start.elapsed();
    println!("nodes = {}", stats.nodes);
    println!("edges = {}", stats.edges);
    println!("wins = {}", stats.wins);
    println!("losses = {}", stats.losses);
    println!("draws = {}", stats.draws);
    println!("audit_seconds = {:.6}", elapsed.as_secs_f64());
    println!(
        "millions_of_edges_per_second = {:.3}",
        stats.edges as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    Ok(())
}

fn verify(
    graph: &PostOpeningGraph,
    rules: Rules,
    path: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    println!("phase = verify");
    let start = Instant::now();
    let state = ParallelState::load(path, graph, rules.stable_tag())?;
    println!("rules_tag = {:#010x}", rules.stable_tag());
    println!("wave = {}", state.wave());
    println!("frontier = {}", state.frontier().len());
    println!(
        "verification_seconds = {:.6}",
        start.elapsed().as_secs_f64()
    );
    Ok(())
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
        "usage:\n  post_opening_solver init <checkpoint.ctb> [threads] [--pawn=travel|outbound|opponent]\n  post_opening_solver verify <checkpoint.ctb> [--pawn=travel|outbound|opponent]\n  post_opening_solver propagate <checkpoint.ctb> [threads] [checkpoint-every-waves] [--pawn=travel|outbound|opponent]\n  post_opening_solver audit <checkpoint.ctb> [threads] [--pawn=travel|outbound|opponent]\n  post_opening_solver opening <checkpoint.ctb> [threads] [--pawn=travel|outbound|opponent]\n  post_opening_solver enrich <checkpoint.ctb> <tablebase.tb> [threads] [--pawn=travel|outbound|opponent]"
    );
    std::process::exit(2)
}
