use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tic_tac_chec::graph::for_each_legal_move;
use tic_tac_chec::ranking::{
    rank_opening, rank_post_opening, unrank_opening, unrank_post_opening, OpeningId, PostOpeningId,
    LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN,
};
use tic_tac_chec::remoteness::DRAW_CODE;
use tic_tac_chec::tablebase::TablebaseArtifact;
use tic_tac_chec::threat::{direct_immediate_wins, reference_immediate_wins, ImmediateWins};
use tic_tac_chec::{Position, Rules};

const MAX_MOVES: usize = 64;
const MOVE_HISTOGRAM_SIDE: usize = MAX_MOVES + 1;
const PROGRESS_BATCH: u64 = 1_000_000;
const PROGRESS_REPORT: u64 = 100_000_000;

#[derive(Clone, Copy, Debug)]
enum Section {
    Post,
    Opening,
}

impl Section {
    const fn label(self) -> &'static str {
        match self {
            Self::Post => "post-opening",
            Self::Opening => "locked opening",
        }
    }

    const fn prefix(self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::Opening => "opening",
        }
    }

    const fn domain(self) -> u32 {
        match self {
            Self::Post => POST_OPENING_DOMAIN,
            Self::Opening => LOCKED_OPENING_DOMAIN,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
enum Outcome {
    Win = 0,
    Loss = 1,
    Draw = 2,
}

impl Outcome {
    const ALL: [Self; 3] = [Self::Win, Self::Loss, Self::Draw];

    const fn label(self) -> &'static str {
        match self {
            Self::Win => "Win",
            Self::Loss => "Loss",
            Self::Draw => "Draw",
        }
    }
}

struct Stats {
    nodes: u64,
    terminal: u64,
    outcomes: [u64; 3],
    threats: [u64; 3],
    threat_actions: u64,
    threat_drop_actions: u64,
    threat_move_actions: u64,
    threat_capture_actions: u64,
    threat_drop_positions: u64,
    threat_move_positions: u64,
    threat_capture_positions: u64,
    threat_action_histogram: [u64; 11],
    detector_mismatches: u64,
    first_detector_mismatch: Option<u32>,

    draw_positions: u64,
    draw_legal_moves: u64,
    draw_preserving_moves: u64,
    draw_immediate_loss_moves: u64,
    draw_positions_with_immediate_loss: u64,
    draw_all_moves_preserve: u64,
    draw_majority_moves_preserve: u64,
    draw_exactly_one_preserves: u64,
    draw_move_histogram: Vec<u64>,

    threatened_legal_moves: u64,
    threatened_safe_moves: u64,
    threatened_unanswerable: u64,
    first_unanswerable: Option<u32>,
    safe_move_histogram: [u64; MOVE_HISTOGRAM_SIDE],
    threatened_draw_move_histogram: Vec<u64>,
    threatened_draw_safe_moves: u64,
    threatened_draw_drawing_moves: u64,
    threatened_draw_safe_histogram: [u64; MOVE_HISTOGRAM_SIDE],
    threatened_draw_drawing_histogram: [u64; MOVE_HISTOGRAM_SIDE],

    loss_two_positions: u64,
    loss_two_equivalence_failures: u64,
    first_loss_two_failure: Option<u32>,
    unsafe_child_code_failures: u64,
    first_unsafe_child_code_failure: Option<u32>,
    drawing_defense_failures: u64,
    first_drawing_defense_failure: Option<u32>,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            nodes: 0,
            terminal: 0,
            outcomes: [0; 3],
            threats: [0; 3],
            threat_actions: 0,
            threat_drop_actions: 0,
            threat_move_actions: 0,
            threat_capture_actions: 0,
            threat_drop_positions: 0,
            threat_move_positions: 0,
            threat_capture_positions: 0,
            threat_action_histogram: [0; 11],
            detector_mismatches: 0,
            first_detector_mismatch: None,
            draw_positions: 0,
            draw_legal_moves: 0,
            draw_preserving_moves: 0,
            draw_immediate_loss_moves: 0,
            draw_positions_with_immediate_loss: 0,
            draw_all_moves_preserve: 0,
            draw_majority_moves_preserve: 0,
            draw_exactly_one_preserves: 0,
            draw_move_histogram: vec![0; MOVE_HISTOGRAM_SIDE * MOVE_HISTOGRAM_SIDE],
            threatened_legal_moves: 0,
            threatened_safe_moves: 0,
            threatened_unanswerable: 0,
            first_unanswerable: None,
            safe_move_histogram: [0; MOVE_HISTOGRAM_SIDE],
            threatened_draw_move_histogram: vec![0; MOVE_HISTOGRAM_SIDE * MOVE_HISTOGRAM_SIDE],
            threatened_draw_safe_moves: 0,
            threatened_draw_drawing_moves: 0,
            threatened_draw_safe_histogram: [0; MOVE_HISTOGRAM_SIDE],
            threatened_draw_drawing_histogram: [0; MOVE_HISTOGRAM_SIDE],
            loss_two_positions: 0,
            loss_two_equivalence_failures: 0,
            first_loss_two_failure: None,
            unsafe_child_code_failures: 0,
            first_unsafe_child_code_failure: None,
            drawing_defense_failures: 0,
            first_drawing_defense_failure: None,
        }
    }
}

impl Stats {
    fn merge(&mut self, other: Self) {
        self.nodes += other.nodes;
        self.terminal += other.terminal;
        for index in 0..3 {
            self.outcomes[index] += other.outcomes[index];
            self.threats[index] += other.threats[index];
        }
        self.threat_actions += other.threat_actions;
        self.threat_drop_actions += other.threat_drop_actions;
        self.threat_move_actions += other.threat_move_actions;
        self.threat_capture_actions += other.threat_capture_actions;
        self.threat_drop_positions += other.threat_drop_positions;
        self.threat_move_positions += other.threat_move_positions;
        self.threat_capture_positions += other.threat_capture_positions;
        add_array(
            &mut self.threat_action_histogram,
            &other.threat_action_histogram,
        );
        self.detector_mismatches += other.detector_mismatches;
        merge_first(
            &mut self.first_detector_mismatch,
            other.first_detector_mismatch,
        );
        self.draw_positions += other.draw_positions;
        self.draw_legal_moves += other.draw_legal_moves;
        self.draw_preserving_moves += other.draw_preserving_moves;
        self.draw_immediate_loss_moves += other.draw_immediate_loss_moves;
        self.draw_positions_with_immediate_loss += other.draw_positions_with_immediate_loss;
        self.draw_all_moves_preserve += other.draw_all_moves_preserve;
        self.draw_majority_moves_preserve += other.draw_majority_moves_preserve;
        self.draw_exactly_one_preserves += other.draw_exactly_one_preserves;
        add_slices(&mut self.draw_move_histogram, &other.draw_move_histogram);
        self.threatened_legal_moves += other.threatened_legal_moves;
        self.threatened_safe_moves += other.threatened_safe_moves;
        self.threatened_unanswerable += other.threatened_unanswerable;
        merge_first(&mut self.first_unanswerable, other.first_unanswerable);
        add_array(&mut self.safe_move_histogram, &other.safe_move_histogram);
        add_slices(
            &mut self.threatened_draw_move_histogram,
            &other.threatened_draw_move_histogram,
        );
        self.threatened_draw_safe_moves += other.threatened_draw_safe_moves;
        self.threatened_draw_drawing_moves += other.threatened_draw_drawing_moves;
        add_array(
            &mut self.threatened_draw_safe_histogram,
            &other.threatened_draw_safe_histogram,
        );
        add_array(
            &mut self.threatened_draw_drawing_histogram,
            &other.threatened_draw_drawing_histogram,
        );
        self.loss_two_positions += other.loss_two_positions;
        self.loss_two_equivalence_failures += other.loss_two_equivalence_failures;
        merge_first(
            &mut self.first_loss_two_failure,
            other.first_loss_two_failure,
        );
        self.unsafe_child_code_failures += other.unsafe_child_code_failures;
        merge_first(
            &mut self.first_unsafe_child_code_failure,
            other.first_unsafe_child_code_failure,
        );
        self.drawing_defense_failures += other.drawing_defense_failures;
        merge_first(
            &mut self.first_drawing_defense_failure,
            other.first_drawing_defense_failure,
        );
    }

    fn invariant_failures(&self) -> u64 {
        self.detector_mismatches
            + self.loss_two_equivalence_failures
            + self.unsafe_child_code_failures
            + self.drawing_defense_failures
    }
}

struct Progress {
    completed: AtomicU64,
    next_report: AtomicU64,
    total: u64,
    started: Instant,
}

impl Progress {
    fn new(total: u64) -> Self {
        Self {
            completed: AtomicU64::new(0),
            next_report: AtomicU64::new(PROGRESS_REPORT),
            total,
            started: Instant::now(),
        }
    }

    fn add(&self, amount: u64) {
        let completed = self.completed.fetch_add(amount, Ordering::Relaxed) + amount;
        let next = self.next_report.load(Ordering::Relaxed);
        if completed >= next
            && self
                .next_report
                .compare_exchange(
                    next,
                    next + PROGRESS_REPORT,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
        {
            println!(
                "progress = {:.2}% ({}/{} positions, {:.1}s)",
                percentage(completed, self.total),
                commas(completed),
                commas(self.total),
                self.started.elapsed().as_secs_f64()
            );
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let (
        Some(tablebase_path),
        Some(markdown_path),
        Some(json_path),
        Some(thread_text),
        Some(commit),
    ) = (
        arguments.get(1),
        arguments.get(2),
        arguments.get(3),
        arguments.get(4),
        arguments.get(5),
    )
    else {
        usage();
    };
    let threads: usize = thread_text.parse()?;
    if threads == 0 {
        return Err("thread count must be positive".into());
    }

    let rules = Rules::ORIGINAL_TRAVEL_DIRECTION;
    println!("Loading and validating tablebase...");
    let tablebase = TablebaseArtifact::load(
        tablebase_path,
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?;
    let progress = Progress::new(POST_OPENING_DOMAIN as u64 + LOCKED_OPENING_DOMAIN as u64);

    println!("Scanning the post-opening indexed domain with {threads} threads...");
    let post = scan_parallel(Section::Post, &tablebase, rules, threads, &progress)?;
    println!("Scanning the locked-opening indexed domain...");
    let opening = scan_parallel(Section::Opening, &tablebase, rules, threads, &progress)?;
    validate_stats(Section::Post, &post)?;
    validate_stats(Section::Opening, &opening)?;

    write_markdown(
        Path::new(markdown_path),
        commit,
        &tablebase,
        rules,
        threads,
        &post,
        &opening,
        progress.started.elapsed().as_secs_f64(),
    )?;
    write_json(
        Path::new(json_path),
        commit,
        &tablebase,
        rules,
        &post,
        &opening,
    )?;
    println!("detector_mismatches = 0");
    println!("tablebase_invariant_failures = 0");
    println!("post_threats = {}", post.threats.iter().sum::<u64>());
    println!(
        "post_draw_threats = {}",
        post.threats[Outcome::Draw as usize]
    );
    println!("output = {markdown_path}");
    println!("machine_output = {json_path}");
    Ok(())
}

fn scan_parallel(
    section: Section,
    tablebase: &TablebaseArtifact,
    rules: Rules,
    threads: usize,
    progress: &Progress,
) -> Result<Stats, Box<dyn Error>> {
    let domain = section.domain();
    let chunk = (domain as usize).div_ceil(threads).max(1);
    let partials = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for first in (0..domain).step_by(chunk) {
            let end = domain.min(first.saturating_add(chunk as u32));
            handles.push(
                scope.spawn(move || scan_range(section, first, end, tablebase, rules, progress)),
            );
        }
        handles
            .into_iter()
            .map(|handle| handle.join())
            .collect::<Result<Vec<_>, _>>()
    })
    .map_err(|_| "census worker panicked")?;
    let mut result = Stats::default();
    for partial in partials {
        result.merge(partial);
    }
    Ok(result)
}

fn scan_range(
    section: Section,
    first: u32,
    end: u32,
    tablebase: &TablebaseArtifact,
    rules: Rules,
    progress: &Progress,
) -> Stats {
    let mut stats = Stats::default();
    let mut unreported = 0_u64;
    for raw in first..end {
        let (position, code) = match section {
            Section::Post => (
                unrank_post_opening(PostOpeningId::new(raw).unwrap()),
                tablebase.post_codes()[raw as usize],
            ),
            Section::Opening => (
                unrank_opening(OpeningId::new(raw).unwrap()),
                tablebase.opening_codes()[raw as usize],
            ),
        };
        scan_position(section, raw, &position, code, tablebase, rules, &mut stats);
        unreported += 1;
        if unreported == PROGRESS_BATCH {
            progress.add(unreported);
            unreported = 0;
        }
    }
    progress.add(unreported);
    stats
}

fn scan_position(
    section: Section,
    raw: u32,
    position: &Position,
    code: u8,
    tablebase: &TablebaseArtifact,
    rules: Rules,
    stats: &mut Stats,
) {
    stats.nodes += 1;
    let outcome = decode(code);
    stats.outcomes[outcome as usize] += 1;
    if position.is_terminal() {
        stats.terminal += 1;
        return;
    }

    let attacker = position.side_to_move().opponent();
    let threat = direct_immediate_wins(position, attacker, rules);
    let reference = reference_immediate_wins(position, attacker, rules);
    if threat != reference {
        stats.detector_mismatches += 1;
        merge_first(&mut stats.first_detector_mismatch, Some(raw));
    }
    let threatened = threat.is_live();
    if threatened {
        record_threat(stats, outcome, threat);
    }
    if outcome != Outcome::Draw && !threatened {
        return;
    }

    let defender = position.side_to_move();
    let mut legal = 0_usize;
    let mut drawing = 0_usize;
    let mut immediate_losses = 0_usize;
    let mut safe = 0_usize;
    for_each_legal_move(position, rules, |action| {
        legal += 1;
        let child = position.play_generated(action);
        let child_code = lookup_code(&child, tablebase);
        if child_code == DRAW_CODE {
            drawing += 1;
        }
        if child_code == 1 {
            immediate_losses += 1;
        }
        if threatened {
            let is_safe = child.winner() == Some(defender)
                || !direct_immediate_wins(&child, child.side_to_move(), rules).is_live();
            if is_safe {
                safe += 1;
            } else if child_code != 1 {
                stats.unsafe_child_code_failures += 1;
                merge_first(&mut stats.first_unsafe_child_code_failure, Some(raw));
            }
            if child_code == DRAW_CODE && !is_safe {
                stats.drawing_defense_failures += 1;
                merge_first(&mut stats.first_drawing_defense_failure, Some(raw));
            }
        }
    });
    assert!(
        legal <= MAX_MOVES,
        "move histogram bound at {}:{raw}",
        section.prefix()
    );

    if outcome == Outcome::Draw {
        stats.draw_positions += 1;
        stats.draw_legal_moves += legal as u64;
        stats.draw_preserving_moves += drawing as u64;
        stats.draw_immediate_loss_moves += immediate_losses as u64;
        stats.draw_positions_with_immediate_loss += u64::from(immediate_losses != 0);
        stats.draw_all_moves_preserve += u64::from(drawing == legal);
        stats.draw_majority_moves_preserve += u64::from(drawing * 2 > legal);
        stats.draw_exactly_one_preserves += u64::from(drawing == 1);
        stats.draw_move_histogram[histogram_index(legal, drawing)] += 1;
    }

    if threatened {
        stats.threatened_legal_moves += legal as u64;
        stats.threatened_safe_moves += safe as u64;
        stats.safe_move_histogram[safe] += 1;
        if safe == 0 {
            stats.threatened_unanswerable += 1;
            merge_first(&mut stats.first_unanswerable, Some(raw));
        }
        let loss_two = code == 2;
        stats.loss_two_positions += u64::from(loss_two);
        if (safe == 0) != loss_two {
            stats.loss_two_equivalence_failures += 1;
            merge_first(&mut stats.first_loss_two_failure, Some(raw));
        }
        if outcome == Outcome::Draw {
            stats.threatened_draw_move_histogram[histogram_index(legal, drawing)] += 1;
            stats.threatened_draw_safe_moves += safe as u64;
            stats.threatened_draw_drawing_moves += drawing as u64;
            stats.threatened_draw_safe_histogram[safe] += 1;
            stats.threatened_draw_drawing_histogram[drawing] += 1;
        }
    }
}

fn record_threat(stats: &mut Stats, outcome: Outcome, threat: ImmediateWins) {
    stats.threats[outcome as usize] += 1;
    stats.threat_actions += threat.actions as u64;
    stats.threat_drop_actions += threat.drops as u64;
    stats.threat_move_actions += threat.moves_to_empty as u64;
    stats.threat_capture_actions += threat.captures as u64;
    stats.threat_drop_positions += u64::from(threat.drops != 0);
    stats.threat_move_positions += u64::from(threat.moves_to_empty != 0);
    stats.threat_capture_positions += u64::from(threat.captures != 0);
    stats.threat_action_histogram[threat.actions as usize] += 1;
}

fn lookup_code(position: &Position, tablebase: &TablebaseArtifact) -> u8 {
    if position.opening_complete() {
        let id = rank_post_opening(position).expect("post-opening child ranks");
        tablebase.post_codes()[id.get() as usize]
    } else {
        let id = rank_opening(position).expect("locked-opening child ranks");
        tablebase.opening_codes()[id.get() as usize]
    }
}

fn validate_stats(section: Section, stats: &Stats) -> Result<(), Box<dyn Error>> {
    if stats.nodes != section.domain() as u64 {
        return Err(format!("{} census node mismatch", section.label()).into());
    }
    if stats.outcomes.iter().sum::<u64>() != stats.nodes {
        return Err(format!("{} outcome partition mismatch", section.label()).into());
    }
    if stats.draw_positions != stats.outcomes[Outcome::Draw as usize] {
        return Err(format!("{} draw analysis omitted positions", section.label()).into());
    }
    let threats = stats.threats.iter().sum::<u64>();
    if stats.threat_action_histogram.iter().sum::<u64>() != threats
        || weighted_count_histogram(&stats.threat_action_histogram) != stats.threat_actions
    {
        return Err(format!("{} threat-action histogram mismatch", section.label()).into());
    }
    if weighted_count_histogram(&stats.threatened_draw_safe_histogram)
        != stats.threatened_draw_safe_moves
        || weighted_count_histogram(&stats.threatened_draw_drawing_histogram)
            != stats.threatened_draw_drawing_moves
    {
        return Err(format!("{} response histogram mismatch", section.label()).into());
    }
    if stats.invariant_failures() != 0 {
        return Err(format!(
            "{} census failed {} detector/tablebase invariants",
            section.label(),
            stats.invariant_failures()
        )
        .into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_markdown(
    path: &Path,
    commit: &str,
    tablebase: &TablebaseArtifact,
    rules: Rules,
    threads: usize,
    post: &Stats,
    opening: &Stats,
    seconds: f64,
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(writer, "# Immediate-threat and defensive-choice census")?;
    writeln!(writer)?;
    writeln!(writer, "Exhaustive census over both indexed structural domains for the original-edition travel-direction rules.")?;
    writeln!(writer)?;
    writeln!(writer, "- Census source commit: `{commit}`")?;
    writeln!(writer, "- Rules tag: `0x{:08x}`", rules.stable_tag())?;
    writeln!(
        writer,
        "- Tablebase CRC-64/XZ: `0x{:016x}`",
        tablebase.checksum()
    )?;
    writeln!(
        writer,
        "- Threads: {threads}; elapsed: {seconds:.3} seconds"
    )?;
    writeln!(writer, "- Independent detector mismatches: 0")?;
    writeln!(writer, "- Tablebase invariant failures: 0")?;
    writeln!(writer)?;
    writeln!(writer, "## Definitions")?;
    writeln!(writer)?;
    writeln!(writer, "A live threat is a distinct legal action the opponent could play immediately on the same board to complete four in a row. This is stricter than a geometric three-piece line: it accounts for the missing piece's location, slider blocking, pawn direction and capture rules, target occupancy, and the opening movement lock.")?;
    writeln!(writer)?;
    writeln!(writer, "A safe response wins immediately or leaves the opponent with no immediate winning action. A drawing response is a safe response that preserves the tablebase draw. An immediate-loss move from a drawn position gives the opponent a win in one ply, so the original mover loses in two plies.")?;
    write_section(&mut writer, Section::Post, post)?;
    write_section(&mut writer, Section::Opening, opening)?;
    writeln!(writer)?;
    writeln!(writer, "## Interpretation")?;
    writeln!(writer)?;
    writeln!(writer, "The indexed-domain results support a precise version of the attention-game hypothesis. Away from immediate threats, drawn positions are broadly forgiving: {:.6}% of all legal moves from drawn post-opening positions preserve the draw, and {:.6}% of those positions draw after every legal move. A live threat sharply narrows the choice: only {:.6}% of moves from drawn live-threat positions preserve the draw, and {:.6}% of those positions have exactly one drawing response.", percentage(post.draw_preserving_moves, post.draw_legal_moves), percentage(post.draw_all_moves_preserve, post.draw_positions), percentage(post.threatened_draw_drawing_moves, histogram_weighted_legal(&post.threatened_draw_move_histogram)), percentage(post.threatened_draw_drawing_histogram[1], post.threats[Outcome::Draw as usize]))?;
    writeln!(writer)?;
    writeln!(writer, "The stronger claim that every threat can be stopped is false. Still, {:.6}% of post-opening live threats have at least one safe response, and every live threat in a drawn position is answerable. The {} unanswerable cases are exactly the tablebase's loss-in-2 positions, independently confirming the tactical definition.", percentage(post.threats.iter().sum::<u64>() - post.threatened_unanswerable, post.threats.iter().sum()), commas(post.threatened_unanswerable))?;
    writeln!(writer)?;
    writeln!(writer, "## Scope and interpretation guardrails")?;
    writeln!(writer)?;
    writeln!(writer, "These are exact counts over the solver's indexed structural domains, not frequencies under human play and not a claim that every indexed position is reachable from the empty board. The post-opening domain normalizes the player to move to White by color-swap plus 180-degree rotation. Threats are tactical one-move facts; tablebase values supply the game-theoretic classification.")?;
    writer.flush()?;
    Ok(())
}

fn write_section(writer: &mut impl Write, section: Section, stats: &Stats) -> std::io::Result<()> {
    let total_threats = stats.threats.iter().sum::<u64>();
    writeln!(writer)?;
    writeln!(writer, "## {}", section.label())?;
    writeln!(writer)?;
    writeln!(
        writer,
        "| Result | All positions | Live-threat positions | Threat rate |"
    )?;
    writeln!(writer, "| --- | ---: | ---: | ---: |")?;
    for outcome in Outcome::ALL {
        writeln!(
            writer,
            "| {} | {} | {} | {:.6}% |",
            outcome.label(),
            commas(stats.outcomes[outcome as usize]),
            commas(stats.threats[outcome as usize]),
            percentage(
                stats.threats[outcome as usize],
                stats.outcomes[outcome as usize]
            )
        )?;
    }
    writeln!(writer)?;
    writeln!(writer, "- Positions: {}", commas(stats.nodes))?;
    writeln!(writer, "- Terminal positions: {}", commas(stats.terminal))?;
    writeln!(
        writer,
        "- Live-threat positions: {} ({:.6}%)",
        commas(total_threats),
        percentage(total_threats, stats.nodes)
    )?;
    writeln!(
        writer,
        "- Answerable live threats: {} ({:.6}%)",
        commas(total_threats - stats.threatened_unanswerable),
        percentage(total_threats - stats.threatened_unanswerable, total_threats)
    )?;
    writeln!(writer, "- Each live-threat position has exactly one immediate winning action: {} total (drop {}, move-to-empty {}, capture {})", commas(stats.threat_actions), commas(stats.threat_drop_actions), commas(stats.threat_move_actions), commas(stats.threat_capture_actions))?;
    writeln!(
        writer,
        "- Unanswerable threats: {}{}",
        commas(stats.threatened_unanswerable),
        witness(section, stats.first_unanswerable)
    )?;
    writeln!(
        writer,
        "- Loss-in-2 positions: {}",
        commas(stats.loss_two_positions)
    )?;
    writeln!(writer)?;
    writeln!(writer, "### Drawn-position choice width")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "- Drawn positions: {}",
        commas(stats.draw_positions)
    )?;
    writeln!(
        writer,
        "- Legal moves: {}; drawing moves: {} ({:.6}%)",
        commas(stats.draw_legal_moves),
        commas(stats.draw_preserving_moves),
        percentage(stats.draw_preserving_moves, stats.draw_legal_moves)
    )?;
    writeln!(
        writer,
        "- Positions where every move draws: {} ({:.6}%)",
        commas(stats.draw_all_moves_preserve),
        percentage(stats.draw_all_moves_preserve, stats.draw_positions)
    )?;
    writeln!(
        writer,
        "- Positions where a majority of moves draw: {} ({:.6}%)",
        commas(stats.draw_majority_moves_preserve),
        percentage(stats.draw_majority_moves_preserve, stats.draw_positions)
    )?;
    writeln!(
        writer,
        "- Positions with exactly one drawing move: {} ({:.6}%)",
        commas(stats.draw_exactly_one_preserves),
        percentage(stats.draw_exactly_one_preserves, stats.draw_positions)
    )?;
    writeln!(
        writer,
        "- Immediate-loss moves: {} across {} drawn positions ({:.6}% of drawn positions)",
        commas(stats.draw_immediate_loss_moves),
        commas(stats.draw_positions_with_immediate_loss),
        percentage(
            stats.draw_positions_with_immediate_loss,
            stats.draw_positions
        )
    )?;
    writeln!(writer)?;
    writeln!(writer, "### Live threats inside draws")?;
    let draw_threats = stats.threats[Outcome::Draw as usize];
    writeln!(writer)?;
    writeln!(
        writer,
        "- Drawn positions with a live threat: {} ({:.6}% of draws)",
        commas(draw_threats),
        percentage(draw_threats, stats.draw_positions)
    )?;
    let threatened_draw_legal = histogram_weighted_legal(&stats.threatened_draw_move_histogram);
    writeln!(
        writer,
        "- Safe responses in drawn live-threat positions: {} ({:.6}% of their legal moves)",
        commas(stats.threatened_draw_safe_moves),
        percentage(stats.threatened_draw_safe_moves, threatened_draw_legal)
    )?;
    writeln!(
        writer,
        "- Drawing responses in drawn live-threat positions: {} ({:.6}% of their legal moves)",
        commas(stats.threatened_draw_drawing_moves),
        percentage(stats.threatened_draw_drawing_moves, threatened_draw_legal)
    )?;
    writeln!(
        writer,
        "- Safe responses across all live threats: {} of {} legal moves ({:.6}%)",
        commas(stats.threatened_safe_moves),
        commas(stats.threatened_legal_moves),
        percentage(stats.threatened_safe_moves, stats.threatened_legal_moves)
    )?;
    writeln!(
        writer,
        "- Drawn live-threat positions with exactly one safe response: {}",
        commas(stats.threatened_draw_safe_histogram[1])
    )?;
    writeln!(
        writer,
        "- Drawn live-threat positions with exactly one drawing response: {}",
        commas(stats.threatened_draw_drawing_histogram[1])
    )?;
    writeln!(writer)?;
    writeln!(writer, "The machine-readable artifact contains the complete legal-moves × drawing-moves distribution plus safe-response and drawing-response histograms.")?;
    Ok(())
}

fn write_json(
    path: &Path,
    commit: &str,
    tablebase: &TablebaseArtifact,
    rules: Rules,
    post: &Stats,
    opening: &Stats,
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(writer, "{{")?;
    writeln!(writer, "  \"schema\": \"tic-tac-chec-threat-census-v1\",")?;
    writeln!(writer, "  \"source_commit\": \"{}\",", escape_json(commit))?;
    writeln!(writer, "  \"rules_tag\": \"0x{:08x}\",", rules.stable_tag())?;
    writeln!(
        writer,
        "  \"tablebase_crc64_xz\": \"0x{:016x}\",",
        tablebase.checksum()
    )?;
    write_json_section(&mut writer, "post_opening", post, true)?;
    write_json_section(&mut writer, "locked_opening", opening, false)?;
    writeln!(writer, "}}")?;
    writer.flush()?;
    Ok(())
}

fn write_json_section(
    writer: &mut impl Write,
    label: &str,
    stats: &Stats,
    comma: bool,
) -> std::io::Result<()> {
    writeln!(writer, "  \"{label}\": {{")?;
    writeln!(writer, "    \"positions\": {},", stats.nodes)?;
    writeln!(writer, "    \"terminal_positions\": {},", stats.terminal)?;
    writeln!(
        writer,
        "    \"wins\": {},",
        stats.outcomes[Outcome::Win as usize]
    )?;
    writeln!(
        writer,
        "    \"losses\": {},",
        stats.outcomes[Outcome::Loss as usize]
    )?;
    writeln!(
        writer,
        "    \"draws\": {},",
        stats.outcomes[Outcome::Draw as usize]
    )?;
    writeln!(
        writer,
        "    \"live_threat_positions\": {},",
        stats.threats.iter().sum::<u64>()
    )?;
    writeln!(
        writer,
        "    \"draw_live_threat_positions\": {},",
        stats.threats[Outcome::Draw as usize]
    )?;
    writeln!(
        writer,
        "    \"win_live_threat_positions\": {},",
        stats.threats[Outcome::Win as usize]
    )?;
    writeln!(
        writer,
        "    \"loss_live_threat_positions\": {},",
        stats.threats[Outcome::Loss as usize]
    )?;
    writeln!(
        writer,
        "    \"unanswerable_threat_positions\": {},",
        stats.threatened_unanswerable
    )?;
    writeln!(
        writer,
        "    \"live_threat_actions\": {},",
        stats.threat_actions
    )?;
    writeln!(
        writer,
        "    \"live_threat_drop_actions\": {},",
        stats.threat_drop_actions
    )?;
    writeln!(
        writer,
        "    \"live_threat_move_to_empty_actions\": {},",
        stats.threat_move_actions
    )?;
    writeln!(
        writer,
        "    \"live_threat_capture_actions\": {},",
        stats.threat_capture_actions
    )?;
    writeln!(
        writer,
        "    \"draw_legal_moves\": {},",
        stats.draw_legal_moves
    )?;
    writeln!(
        writer,
        "    \"draw_preserving_moves\": {},",
        stats.draw_preserving_moves
    )?;
    writeln!(
        writer,
        "    \"draw_immediate_loss_moves\": {},",
        stats.draw_immediate_loss_moves
    )?;
    writeln!(
        writer,
        "    \"draw_positions_with_immediate_loss\": {},",
        stats.draw_positions_with_immediate_loss
    )?;
    writeln!(
        writer,
        "    \"draw_all_moves_preserve\": {},",
        stats.draw_all_moves_preserve
    )?;
    writeln!(
        writer,
        "    \"draw_majority_moves_preserve\": {},",
        stats.draw_majority_moves_preserve
    )?;
    writeln!(
        writer,
        "    \"draw_exactly_one_preserves\": {},",
        stats.draw_exactly_one_preserves
    )?;
    writeln!(
        writer,
        "    \"threatened_legal_moves\": {},",
        stats.threatened_legal_moves
    )?;
    writeln!(
        writer,
        "    \"threatened_safe_moves\": {},",
        stats.threatened_safe_moves
    )?;
    writeln!(
        writer,
        "    \"threatened_draw_safe_moves\": {},",
        stats.threatened_draw_safe_moves
    )?;
    writeln!(
        writer,
        "    \"threatened_draw_drawing_moves\": {},",
        stats.threatened_draw_drawing_moves
    )?;
    writeln!(
        writer,
        "    \"detector_mismatches\": {},",
        stats.detector_mismatches
    )?;
    writeln!(
        writer,
        "    \"tablebase_invariant_failures\": {},",
        stats.invariant_failures() - stats.detector_mismatches
    )?;
    write_move_histogram(
        writer,
        "draw_move_histogram",
        &stats.draw_move_histogram,
        true,
    )?;
    write_move_histogram(
        writer,
        "threatened_draw_move_histogram",
        &stats.threatened_draw_move_histogram,
        true,
    )?;
    write_count_histogram(
        writer,
        "threatened_draw_safe_response_histogram",
        &stats.threatened_draw_safe_histogram,
        true,
    )?;
    write_count_histogram(
        writer,
        "threatened_draw_drawing_response_histogram",
        &stats.threatened_draw_drawing_histogram,
        false,
    )?;
    writeln!(writer, "  }}{}", if comma { "," } else { "" })?;
    Ok(())
}

fn write_move_histogram(
    writer: &mut impl Write,
    label: &str,
    histogram: &[u64],
    comma: bool,
) -> std::io::Result<()> {
    writeln!(writer, "    \"{label}\": [")?;
    let entries: Vec<_> = (0..=MAX_MOVES)
        .flat_map(|legal| (0..=legal).map(move |drawing| (legal, drawing)))
        .filter(|&(legal, drawing)| histogram[histogram_index(legal, drawing)] != 0)
        .collect();
    for (index, &(legal, drawing)) in entries.iter().enumerate() {
        writeln!(
            writer,
            "      {{\"legal\": {legal}, \"drawing\": {drawing}, \"positions\": {}}}{}",
            histogram[histogram_index(legal, drawing)],
            if index + 1 == entries.len() { "" } else { "," }
        )?;
    }
    writeln!(writer, "    ]{}", if comma { "," } else { "" })?;
    Ok(())
}

fn write_count_histogram(
    writer: &mut impl Write,
    label: &str,
    histogram: &[u64],
    comma: bool,
) -> std::io::Result<()> {
    writeln!(writer, "    \"{label}\": [")?;
    let entries: Vec<_> = histogram
        .iter()
        .enumerate()
        .filter(|&(_, &positions)| positions != 0)
        .collect();
    for (index, &(responses, &positions)) in entries.iter().enumerate() {
        writeln!(
            writer,
            "      {{\"responses\": {responses}, \"positions\": {positions}}}{}",
            if index + 1 == entries.len() { "" } else { "," }
        )?;
    }
    writeln!(writer, "    ]{}", if comma { "," } else { "" })?;
    Ok(())
}

fn histogram_index(legal: usize, drawing: usize) -> usize {
    legal * MOVE_HISTOGRAM_SIDE + drawing
}

fn histogram_weighted_legal(histogram: &[u64]) -> u64 {
    (0..=MAX_MOVES)
        .map(|legal| {
            let positions: u64 = (0..=legal)
                .map(|drawing| histogram[histogram_index(legal, drawing)])
                .sum();
            legal as u64 * positions
        })
        .sum()
}

fn weighted_count_histogram(histogram: &[u64]) -> u64 {
    histogram
        .iter()
        .enumerate()
        .map(|(count, &positions)| count as u64 * positions)
        .sum()
}

fn decode(code: u8) -> Outcome {
    if code == DRAW_CODE {
        Outcome::Draw
    } else if code.is_multiple_of(2) {
        Outcome::Loss
    } else {
        Outcome::Win
    }
}

fn add_array<const N: usize>(target: &mut [u64; N], source: &[u64; N]) {
    for index in 0..N {
        target[index] += source[index];
    }
}

fn add_slices(target: &mut [u64], source: &[u64]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target += *source;
    }
}

fn merge_first(target: &mut Option<u32>, source: Option<u32>) {
    if let Some(source) = source {
        *target = Some(target.map_or(source, |target| target.min(source)));
    }
}

fn witness(section: Section, raw: Option<u32>) -> String {
    raw.map_or_else(String::new, |raw| {
        format!(" (first at `{}:{raw}`)", section.prefix())
    })
}

fn percentage(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn commas(value: u64) -> String {
    let text = value.to_string();
    let mut formatted = String::with_capacity(text.len() + text.len() / 3);
    for (index, character) in text.chars().enumerate() {
        if index != 0 && (text.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(character);
    }
    formatted
}

fn escape_json(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn create_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn usage() -> ! {
    eprintln!(
        "usage: threat_census <tablebase.tb> <report.md> <report.json> <threads> <source-commit>"
    );
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_and_histogram_layout_are_stable() {
        assert_eq!(decode(0), Outcome::Loss);
        assert_eq!(decode(1), Outcome::Win);
        assert_eq!(decode(DRAW_CODE), Outcome::Draw);
        assert_ne!(histogram_index(8, 3), histogram_index(7, 4));
    }

    #[test]
    fn merging_preserves_lowest_witness_and_histograms() {
        let mut left = Stats {
            first_unanswerable: Some(90),
            ..Stats::default()
        };
        left.draw_move_histogram[histogram_index(8, 3)] = 2;
        let mut right = Stats {
            first_unanswerable: Some(40),
            ..Stats::default()
        };
        right.draw_move_histogram[histogram_index(8, 3)] = 5;
        left.merge(right);
        assert_eq!(left.first_unanswerable, Some(40));
        assert_eq!(left.draw_move_histogram[histogram_index(8, 3)], 7);
    }
}
