use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use tic_tac_chec::probe::{probe, PositionKey, ProbedMove};
use tic_tac_chec::ranking::{
    unrank_opening, unrank_post_opening, OpeningId, PostOpeningId, LOCKED_OPENING_DOMAIN,
    POST_OPENING_DOMAIN,
};
use tic_tac_chec::remoteness::DRAW_CODE;
use tic_tac_chec::retrograde::Value;
use tic_tac_chec::tablebase::TablebaseArtifact;
use tic_tac_chec::{
    Color, Move, PawnDirection, Piece, PieceKind, Position, Rules, Square, BOARD_SIDE,
};

const POLICY: &str = "least-drawing-action-v1";

#[derive(Clone, Copy, Debug)]
enum Section {
    Opening,
    PostOpening,
}

impl Section {
    const fn label(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::PostOpening => "post-opening",
        }
    }

    const fn id_prefix(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::PostOpening => "post",
        }
    }
}

struct Census {
    counts: [u64; 256],
    first_ids: [Option<u32>; 256],
}

impl Census {
    fn from_codes(codes: &[u8]) -> Self {
        let mut counts = [0_u64; 256];
        let mut first_ids = [None; 256];
        for (raw, &code) in codes.iter().enumerate() {
            counts[code as usize] += 1;
            first_ids[code as usize].get_or_insert(raw as u32);
        }
        Self { counts, first_ids }
    }

    fn value_count(&self, value: Value) -> u64 {
        match value {
            Value::Draw => self.counts[DRAW_CODE as usize],
            Value::Win => (1..DRAW_CODE)
                .step_by(2)
                .map(|code| self.counts[code as usize])
                .sum(),
            Value::Loss => (0..DRAW_CODE)
                .step_by(2)
                .map(|code| self.counts[code as usize])
                .sum(),
        }
    }

    fn maximum_code(&self, value: Value) -> Option<u8> {
        (0..DRAW_CODE)
            .rev()
            .find(|&code| code_value(code) == value && self.counts[code as usize] != 0)
    }

    fn first_id(&self, code: u8) -> u32 {
        self.first_ids[code as usize].expect("reported distance has a representative")
    }
}

#[derive(Debug)]
struct LineStep {
    ply: usize,
    position: PositionKey,
    side_to_move: Color,
    action: Move,
}

#[derive(Debug)]
struct DecisiveLine {
    section: Section,
    start_id: u32,
    start: Position,
    value: Value,
    distance: u8,
    steps: Vec<LineStep>,
    terminal: PositionKey,
    winner: Color,
}

#[derive(Clone, Debug)]
struct CriticalDrawChoice {
    ply: usize,
    position_key: PositionKey,
    position: Position,
    chosen: ProbedMove,
    drawing_moves: Vec<ProbedMove>,
    losing_moves: Vec<ProbedMove>,
}

struct DrawingAnalysis {
    prefix_plies: usize,
    cycle_plies: usize,
    earliest_risk: CriticalDrawChoice,
    narrowest_defense: CriticalDrawChoice,
    most_losing_alternatives: CriticalDrawChoice,
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(tablebase_path) = arguments.get(1) else {
        usage();
    };
    let Some(output_path) = arguments.get(2) else {
        usage();
    };
    let Some(source_commit) = arguments.get(3) else {
        usage();
    };

    let rules = Rules::ORIGINAL_TRAVEL_DIRECTION;
    println!("Loading and validating tablebase...");
    let tablebase = TablebaseArtifact::load(
        Path::new(tablebase_path),
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?;

    println!("Scanning exact value and remoteness distributions...");
    let post_census = Census::from_codes(tablebase.post_codes());
    let opening_census = Census::from_codes(tablebase.opening_codes());
    validate_census(&post_census, tablebase.post_codes().len())?;
    validate_census(&opening_census, tablebase.opening_codes().len())?;

    println!("Extracting and replaying representative decisive lines...");
    let lines = [
        representative_line(
            Section::PostOpening,
            Value::Win,
            &post_census,
            &tablebase,
            rules,
        )?,
        representative_line(
            Section::PostOpening,
            Value::Loss,
            &post_census,
            &tablebase,
            rules,
        )?,
        representative_line(
            Section::Opening,
            Value::Win,
            &opening_census,
            &tablebase,
            rules,
        )?,
        representative_line(
            Section::Opening,
            Value::Loss,
            &opening_census,
            &tablebase,
            rules,
        )?,
    ];

    println!("Auditing critical choices on the canonical drawing lasso...");
    let drawing = analyze_drawing_lasso(&tablebase, rules)?;
    write_report(
        Path::new(output_path),
        source_commit,
        &tablebase,
        rules,
        &post_census,
        &opening_census,
        &lines,
        &drawing,
    )?;

    println!("post_nodes = {}", tablebase.post_codes().len());
    println!("opening_nodes = {}", tablebase.opening_codes().len());
    for line in &lines {
        println!(
            "{}_max_{:?} = {} plies at {}:{}",
            line.section.label(),
            line.value,
            line.distance,
            line.section.id_prefix(),
            line.start_id
        );
    }
    println!("drawing_prefix_plies = {}", drawing.prefix_plies);
    println!("drawing_cycle_plies = {}", drawing.cycle_plies);
    println!("replay_audits = passed");
    println!("output = {output_path}");
    Ok(())
}

fn validate_census(census: &Census, expected: usize) -> Result<(), Box<dyn Error>> {
    let total: u64 = census.counts.iter().sum();
    if total != expected as u64 {
        return Err(format!("census has {total} nodes; expected {expected}").into());
    }
    if census.counts[254] != 0 {
        return Err("tablebase census contains reserved code 254".into());
    }
    Ok(())
}

fn representative_line(
    section: Section,
    value: Value,
    census: &Census,
    tablebase: &TablebaseArtifact,
    rules: Rules,
) -> Result<DecisiveLine, Box<dyn Error>> {
    let distance = census
        .maximum_code(value)
        .ok_or_else(|| format!("{} section has no {value:?} position", section.label()))?;
    let start_id = census.first_id(distance);
    let start = position(section, start_id)?;
    extract_decisive_line(section, start_id, start, tablebase, rules)
}

fn extract_decisive_line(
    section: Section,
    start_id: u32,
    start: Position,
    tablebase: &TablebaseArtifact,
    rules: Rules,
) -> Result<DecisiveLine, Box<dyn Error>> {
    let initial_probe = probe(&start, rules, tablebase)?;
    let value = initial_probe.outcome.value;
    let distance = initial_probe
        .outcome
        .distance
        .ok_or("representative decisive line starts in a draw")?;
    let mut position = start.clone();
    let mut steps = Vec::with_capacity(distance as usize);

    for ply in 0..distance as usize {
        let result = probe(&position, rules, tablebase)?;
        let action = result
            .moves
            .iter()
            .filter(|candidate| candidate.optimal)
            .min_by_key(|candidate| action_key(candidate.action))
            .ok_or_else(|| {
                format!(
                    "decisive position {} has no optimal action",
                    result.position
                )
            })?
            .action;
        steps.push(LineStep {
            ply,
            position: result.position,
            side_to_move: position.side_to_move(),
            action,
        });
        position = position.play(action, rules)?;
    }

    if !position.is_terminal() {
        return Err(format!("{distance}-ply principal line did not terminate").into());
    }
    let terminal_probe = probe(&position, rules, tablebase)?;
    if terminal_probe.outcome.value != Value::Loss
        || terminal_probe.outcome.distance != Some(0)
        || !terminal_probe.moves.is_empty()
    {
        return Err("principal line ended at an invalid terminal table entry".into());
    }
    let winner = position
        .winner()
        .ok_or("terminal principal line has no winner")?;
    if steps.len() != distance as usize {
        return Err("principal-line length differs from tablebase remoteness".into());
    }

    Ok(DecisiveLine {
        section,
        start_id,
        start,
        value,
        distance,
        steps,
        terminal: terminal_probe.position,
        winner,
    })
}

fn analyze_drawing_lasso(
    tablebase: &TablebaseArtifact,
    rules: Rules,
) -> Result<DrawingAnalysis, Box<dyn Error>> {
    let mut position = Position::initial();
    let mut visited = HashMap::new();
    let mut choices = Vec::new();
    visited.insert(position.clone(), 0_usize);

    let (prefix_plies, total_plies) = loop {
        let result = probe(&position, rules, tablebase)?;
        if result.outcome.value != Value::Draw {
            return Err(format!("drawing policy reached {:?}", result.outcome.value).into());
        }
        let chosen = result
            .moves
            .iter()
            .filter(|candidate| candidate.outcome.value == Value::Draw)
            .min_by_key(|candidate| action_key(candidate.action))
            .cloned()
            .ok_or("drawn position has no drawing continuation")?;
        let mut drawing_moves: Vec<_> = result
            .moves
            .iter()
            .filter(|candidate| candidate.outcome.value == Value::Draw)
            .cloned()
            .collect();
        let mut losing_moves: Vec<_> = result
            .moves
            .iter()
            .filter(|candidate| candidate.outcome.value == Value::Loss)
            .cloned()
            .collect();
        if result
            .moves
            .iter()
            .any(|candidate| candidate.outcome.value == Value::Win)
        {
            return Err("drawn position unexpectedly has a winning action".into());
        }
        drawing_moves.sort_by_key(|candidate| action_key(candidate.action));
        losing_moves.sort_by_key(|candidate| action_key(candidate.action));
        choices.push(CriticalDrawChoice {
            ply: choices.len(),
            position_key: result.position,
            position: position.clone(),
            chosen: chosen.clone(),
            drawing_moves,
            losing_moves,
        });
        let child = position.play(chosen.action, rules)?;
        if let Some(&cycle_start) = visited.get(&child) {
            break (cycle_start, choices.len());
        }
        visited.insert(child.clone(), choices.len());
        position = child;
    };

    let risky: Vec<_> = choices
        .iter()
        .filter(|choice| !choice.losing_moves.is_empty())
        .cloned()
        .collect();
    let earliest_risk = risky
        .first()
        .cloned()
        .ok_or("drawing lasso has no losing deviations")?;
    let narrowest_defense = risky
        .iter()
        .min_by_key(|choice| (choice.drawing_moves.len(), choice.ply))
        .cloned()
        .expect("risky choices are nonempty");
    let most_losing_alternatives = risky
        .iter()
        .max_by_key(|choice| (choice.losing_moves.len(), usize::MAX - choice.ply))
        .cloned()
        .expect("risky choices are nonempty");

    Ok(DrawingAnalysis {
        prefix_plies,
        cycle_plies: total_plies - prefix_plies,
        earliest_risk,
        narrowest_defense,
        most_losing_alternatives,
    })
}

#[allow(clippy::too_many_arguments)]
fn write_report(
    path: &Path,
    source_commit: &str,
    tablebase: &TablebaseArtifact,
    rules: Rules,
    post: &Census,
    opening: &Census,
    lines: &[DecisiveLine; 4],
    drawing: &DrawingAnalysis,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(writer, "# Canonical strategic report")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "Generated from the complete result-plus-remoteness tablebase."
    )?;
    writeln!(writer)?;
    writeln!(writer, "- Extractor source commit: `{source_commit}`")?;
    writeln!(
        writer,
        "- Rules: original Dream Green edition, travel-direction pawn captures"
    )?;
    writeln!(writer, "- Rules tag: `0x{:08x}`", rules.stable_tag())?;
    writeln!(
        writer,
        "- Tablebase CRC-64/XZ: `0x{:016x}`",
        tablebase.checksum()
    )?;
    writeln!(
        writer,
        "- Post-opening positions: {}",
        commas(POST_OPENING_DOMAIN as u64)
    )?;
    writeln!(
        writer,
        "- Locked-opening positions: {}",
        commas(LOCKED_OPENING_DOMAIN as u64)
    )?;
    writeln!(writer)?;
    writeln!(writer, "## Exact census")?;
    writeln!(writer)?;
    writeln!(writer, "| Section | Wins | Losses | Draws |")?;
    writeln!(writer, "| --- | ---: | ---: | ---: |")?;
    write_census_row(&mut writer, "Post-opening", post)?;
    write_census_row(&mut writer, "Locked opening", opening)?;
    writeln!(writer)?;
    writeln!(writer, "### Decisive remoteness distribution")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "Distance is measured in plies to a terminal four-in-a-row under fastest-win/longest-loss play."
    )?;
    writeln!(writer)?;
    writeln!(writer, "| Plies | Result | Post-opening | Locked opening |")?;
    writeln!(writer, "| ---: | --- | ---: | ---: |")?;
    for distance in 0..DRAW_CODE {
        let post_count = post.counts[distance as usize];
        let opening_count = opening.counts[distance as usize];
        if post_count == 0 && opening_count == 0 {
            continue;
        }
        writeln!(
            writer,
            "| {distance} | {:?} | {} | {} |",
            code_value(distance),
            commas(post_count),
            commas(opening_count)
        )?;
    }

    writeln!(writer)?;
    writeln!(writer, "## Representative longest decisive lines")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "For each section and result, this selects the lowest dense ID at that result's maximum remoteness. Tied optimal actions use the same deterministic action ordering as the drawing witness. Each line was replayed through checked move application and ends at a terminal loss code at exactly the advertised distance."
    )?;
    for line in lines {
        write_line(&mut writer, line)?;
    }

    writeln!(writer)?;
    writeln!(writer, "## Critical choices on one drawing lasso")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "The `{POLICY}` line has a {}-ply prefix and a {}-ply exact cycle. These positions illustrate where that one deterministic drawing line becomes tactically unforgiving; they are not claims about frequency across all play or a standalone proof of the draw.",
        drawing.prefix_plies, drawing.cycle_plies
    )?;
    write_draw_choice(
        &mut writer,
        "Earliest losing deviation",
        &drawing.earliest_risk,
    )?;
    if drawing.narrowest_defense.ply != drawing.earliest_risk.ply {
        write_draw_choice(
            &mut writer,
            "Narrowest drawing defense",
            &drawing.narrowest_defense,
        )?;
    }
    if drawing.most_losing_alternatives.ply != drawing.earliest_risk.ply
        && drawing.most_losing_alternatives.ply != drawing.narrowest_defense.ply
    {
        write_draw_choice(
            &mut writer,
            "Most losing alternatives",
            &drawing.most_losing_alternatives,
        )?;
    }

    writeln!(writer)?;
    writeln!(writer, "## Interpretation limits")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "The census is exhaustive over the solver's dense structural domains. The principal variations and drawing choices are deterministic examples, not unique strategies. Dense post-opening IDs normalize the player to move to White; the full engine positions used for lasso repetition retain absolute color, side to move, opening phase, and both pawn directions. The independently audited tablebase—not these selected lines—is the strong-solution proof."
    )?;
    writer.flush()?;
    Ok(())
}

fn write_census_row(writer: &mut impl Write, label: &str, census: &Census) -> std::io::Result<()> {
    writeln!(
        writer,
        "| {label} | {} | {} | {} |",
        commas(census.value_count(Value::Win)),
        commas(census.value_count(Value::Loss)),
        commas(census.value_count(Value::Draw))
    )
}

fn write_line(writer: &mut impl Write, line: &DecisiveLine) -> std::io::Result<()> {
    writeln!(writer)?;
    writeln!(
        writer,
        "### {} {:?}: {} plies",
        line.section.label(),
        line.value,
        line.distance
    )?;
    writeln!(writer)?;
    writeln!(
        writer,
        "- Start: `{}:{}`",
        line.section.id_prefix(),
        line.start_id
    )?;
    writeln!(writer, "- Side to move: {:?}", line.start.side_to_move())?;
    writeln!(
        writer,
        "- Terminal: `{}`; winner: {:?}",
        line.terminal, line.winner
    )?;
    write_position(writer, &line.start)?;
    writeln!(writer)?;
    for chunk in line.steps.chunks(8) {
        let rendered = chunk
            .iter()
            .map(|step| {
                format!(
                    "{}. {:?} {} (`{}`)",
                    step.ply + 1,
                    step.side_to_move,
                    step.action,
                    step.position
                )
            })
            .collect::<Vec<_>>()
            .join(" · ");
        writeln!(writer, "{rendered}  ")?;
    }
    Ok(())
}

fn write_draw_choice(
    writer: &mut impl Write,
    label: &str,
    choice: &CriticalDrawChoice,
) -> std::io::Result<()> {
    writeln!(writer)?;
    writeln!(writer, "### {label}: ply {}", choice.ply)?;
    writeln!(writer)?;
    writeln!(writer, "- Position: `{}`", choice.position_key)?;
    writeln!(
        writer,
        "- Side to move: {:?}",
        choice.position.side_to_move()
    )?;
    writeln!(writer, "- Policy move: `{}`", choice.chosen.action)?;
    writeln!(
        writer,
        "- Drawing choices: {} of {} legal moves",
        choice.drawing_moves.len(),
        choice.drawing_moves.len() + choice.losing_moves.len()
    )?;
    writeln!(writer, "- Losing choices: {}", choice.losing_moves.len())?;
    write_position(writer, &choice.position)?;
    writeln!(writer)?;
    writeln!(
        writer,
        "Drawing: {}  ",
        move_list(&choice.drawing_moves, false)
    )?;
    writeln!(
        writer,
        "Losing: {}  ",
        move_list(&choice.losing_moves, true)
    )?;
    Ok(())
}

fn write_position(writer: &mut impl Write, position: &Position) -> std::io::Result<()> {
    writeln!(writer)?;
    writeln!(writer, "```text")?;
    for rank in (0..BOARD_SIDE).rev() {
        write!(writer, "{} ", rank + 1)?;
        for file in 0..BOARD_SIDE {
            let square = Square::new(file, rank).expect("board coordinate is valid");
            let symbol = position.at(square).map_or('.', piece_symbol);
            write!(writer, "{symbol} ")?;
        }
        writeln!(writer)?;
    }
    writeln!(writer, "  a b c d")?;
    writeln!(writer, "White hand: {}", hand(position, Color::White))?;
    writeln!(writer, "Black hand: {}", hand(position, Color::Black))?;
    writeln!(
        writer,
        "Pawn directions: White {}, Black {}",
        pawn_arrow(position.pawn_direction(Color::White)),
        pawn_arrow(position.pawn_direction(Color::Black))
    )?;
    writeln!(writer, "```")?;
    Ok(())
}

fn move_list(moves: &[ProbedMove], include_distance: bool) -> String {
    moves
        .iter()
        .map(|candidate| {
            if include_distance {
                format!(
                    "`{}` (loss in {})",
                    candidate.action,
                    candidate
                        .outcome
                        .distance
                        .map_or_else(|| "draw".to_owned(), |distance| format!("{distance} plies"))
                )
            } else {
                format!("`{}`", candidate.action)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn hand(position: &Position, color: Color) -> String {
    let pieces: String = PieceKind::ALL
        .into_iter()
        .filter(|&kind| position.piece_square(Piece { color, kind }).is_none())
        .map(piece_kind_symbol)
        .collect();
    if pieces.is_empty() {
        "—".to_owned()
    } else {
        pieces
    }
}

fn piece_symbol(piece: Piece) -> char {
    let symbol = piece_kind_symbol(piece.kind);
    if piece.color == Color::White {
        symbol
    } else {
        symbol.to_ascii_lowercase()
    }
}

const fn piece_kind_symbol(kind: PieceKind) -> char {
    match kind {
        PieceKind::Pawn => 'P',
        PieceKind::Knight => 'N',
        PieceKind::Bishop => 'B',
        PieceKind::Rook => 'R',
    }
}

const fn pawn_arrow(direction: PawnDirection) -> char {
    match direction {
        PawnDirection::TowardWhite => '↓',
        PawnDirection::TowardBlack => '↑',
    }
}

fn position(section: Section, raw: u32) -> Result<Position, Box<dyn Error>> {
    match section {
        Section::Opening => OpeningId::new(raw)
            .map(unrank_opening)
            .ok_or_else(|| format!("opening ID {raw} is out of range").into()),
        Section::PostOpening => PostOpeningId::new(raw)
            .map(unrank_post_opening)
            .ok_or_else(|| format!("post-opening ID {raw} is out of range").into()),
    }
}

fn code_value(code: u8) -> Value {
    match code {
        DRAW_CODE => Value::Draw,
        distance if distance.is_multiple_of(2) => Value::Loss,
        _ => Value::Win,
    }
}

fn action_key(action: Move) -> (u8, u8, u8) {
    match action {
        Move::Place { piece, to } => (0, piece as u8, to.index() as u8),
        Move::Move { from, to } => (1, from.index() as u8, to.index() as u8),
    }
}

fn commas(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn usage() -> ! {
    eprintln!("usage: strategic_report <tablebase.tb> <output.md> <source-commit>");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_decodes_parity_and_draws() {
        let census = Census::from_codes(&[0, 0, 1, 2, 3, DRAW_CODE, DRAW_CODE]);
        assert_eq!(census.value_count(Value::Loss), 3);
        assert_eq!(census.value_count(Value::Win), 2);
        assert_eq!(census.value_count(Value::Draw), 2);
        assert_eq!(census.maximum_code(Value::Loss), Some(2));
        assert_eq!(census.maximum_code(Value::Win), Some(3));
        assert_eq!(census.first_id(2), 3);
    }

    #[test]
    fn comma_formatter_handles_boundaries() {
        assert_eq!(commas(0), "0");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1_000), "1,000");
        assert_eq!(commas(2_462_360_745), "2,462,360,745");
    }
}
