use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use tic_tac_chec::probe::{probe, PositionKey, ProbedMove};
use tic_tac_chec::ranking::{LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN};
use tic_tac_chec::retrograde::Value;
use tic_tac_chec::tablebase::TablebaseArtifact;
use tic_tac_chec::{Color, Move, Position, Rules};

const POLICY: &str = "least-drawing-action-v1";
const DEFAULT_MAX_PLIES: usize = 1_000_000;

#[derive(Debug)]
struct WitnessStep {
    ply: usize,
    position: PositionKey,
    side_to_move: Color,
    move_index: usize,
    action: Move,
    child: PositionKey,
    legal_moves: usize,
    winning_moves: usize,
    drawing_moves: usize,
    losing_moves: usize,
}

#[derive(Debug)]
struct Witness {
    steps: Vec<WitnessStep>,
    cycle_start: usize,
    repeated_position: PositionKey,
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(tablebase_path) = arguments.get(1) else {
        usage();
    };
    let Some(output_path) = arguments.get(2) else {
        usage();
    };
    let max_plies = arguments
        .get(3)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(DEFAULT_MAX_PLIES);
    if max_plies == 0 {
        return Err("max plies must be positive".into());
    }

    let rules = Rules::ORIGINAL_TRAVEL_DIRECTION;
    println!("Loading and validating tablebase...");
    let tablebase = TablebaseArtifact::load(
        Path::new(tablebase_path),
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?;
    println!("Extracting deterministic drawing lasso (limit {max_plies} plies)...");
    let witness = extract(&tablebase, rules, max_plies)?;
    audit(&witness, &tablebase, rules)?;
    write_json(
        Path::new(output_path),
        &witness,
        &tablebase,
        rules,
        max_plies,
    )?;

    println!("policy = {POLICY}");
    println!("prefix_plies = {}", witness.cycle_start);
    println!(
        "cycle_plies = {}",
        witness.steps.len() - witness.cycle_start
    );
    println!("total_plies = {}", witness.steps.len());
    println!("repeated_position = {}", witness.repeated_position);
    println!("replay_audit = passed");
    println!("output = {output_path}");
    Ok(())
}

fn audit(
    witness: &Witness,
    tablebase: &TablebaseArtifact,
    rules: Rules,
) -> Result<(), Box<dyn Error>> {
    let mut position = Position::initial();
    let mut positions = vec![position.clone()];
    for (ply, step) in witness.steps.iter().enumerate() {
        let result = probe(&position, rules, tablebase)?;
        if step.ply != ply
            || result.position != step.position
            || result.outcome.value != Value::Draw
            || position.side_to_move() != step.side_to_move
            || result.moves.len() != step.legal_moves
        {
            return Err(format!("witness metadata mismatch at ply {ply}").into());
        }
        let (policy_index, policy_move) = choose_drawing_move(&result.moves)
            .ok_or_else(|| format!("replayed draw at ply {ply} has no drawing move"))?;
        let Some(indexed_move) = result.moves.get(step.move_index) else {
            return Err(format!("move index is out of range at ply {ply}").into());
        };
        if policy_index != step.move_index
            || policy_move.action != step.action
            || indexed_move.action != step.action
            || indexed_move.child != step.child
            || indexed_move.outcome.value != Value::Draw
        {
            return Err(format!("witness action mismatch at ply {ply}").into());
        }
        let (winning_moves, drawing_moves, losing_moves) = outcome_counts(&result.moves);
        if (winning_moves, drawing_moves, losing_moves)
            != (step.winning_moves, step.drawing_moves, step.losing_moves)
        {
            return Err(format!("witness alternative counts mismatch at ply {ply}").into());
        }
        position = position
            .play(step.action, rules)
            .expect("audited legal action remains playable");
        positions.push(position.clone());
    }
    let Some(cycle_position) = positions.get(witness.cycle_start) else {
        return Err("cycle start is outside the replayed line".into());
    };
    if &position != cycle_position {
        return Err("final position does not exactly repeat the cycle start".into());
    }
    let final_key = probe(&position, rules, tablebase)?.position;
    if final_key != witness.repeated_position {
        return Err("repeated position key does not match the final position".into());
    }
    Ok(())
}

fn extract(
    tablebase: &TablebaseArtifact,
    rules: Rules,
    max_plies: usize,
) -> Result<Witness, Box<dyn Error>> {
    let mut position = Position::initial();
    let mut visited = HashMap::new();
    let mut steps = Vec::new();
    visited.insert(position.clone(), 0_usize);

    for ply in 0..max_plies {
        let result = probe(&position, rules, tablebase)?;
        if result.outcome.value != Value::Draw {
            return Err(format!(
                "policy left the draw region at ply {ply}: {} is {:?}",
                result.position, result.outcome.value
            )
            .into());
        }
        let (move_index, chosen) = choose_drawing_move(&result.moves)
            .ok_or_else(|| format!("drawn position {} has no drawing move", result.position))?;
        let (winning_moves, drawing_moves, losing_moves) = outcome_counts(&result.moves);
        let child_position = position
            .play(chosen.action, rules)
            .expect("probed legal action remains playable");
        let step = WitnessStep {
            ply,
            position: result.position,
            side_to_move: position.side_to_move(),
            move_index,
            action: chosen.action,
            child: chosen.child,
            legal_moves: result.moves.len(),
            winning_moves,
            drawing_moves,
            losing_moves,
        };
        steps.push(step);

        if let Some(&cycle_start) = visited.get(&child_position) {
            return Ok(Witness {
                steps,
                cycle_start,
                repeated_position: chosen.child,
            });
        }
        visited.insert(child_position.clone(), steps.len());
        position = child_position;
    }

    Err(format!("no exact position repeated within {max_plies} plies").into())
}

fn outcome_counts(moves: &[ProbedMove]) -> (usize, usize, usize) {
    let winning = moves
        .iter()
        .filter(|candidate| candidate.outcome.value == Value::Win)
        .count();
    let drawing = moves
        .iter()
        .filter(|candidate| candidate.outcome.value == Value::Draw)
        .count();
    (winning, drawing, moves.len() - winning - drawing)
}

fn choose_drawing_move(moves: &[ProbedMove]) -> Option<(usize, &ProbedMove)> {
    moves
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.outcome.value == Value::Draw)
        .min_by_key(|(_, candidate)| action_key(candidate.action))
}

fn action_key(action: Move) -> (u8, u8, u8) {
    match action {
        Move::Place { piece, to } => (0, piece as u8, to.index() as u8),
        Move::Move { from, to } => (1, from.index() as u8, to.index() as u8),
    }
}

fn write_json(
    path: &Path,
    witness: &Witness,
    tablebase: &TablebaseArtifact,
    rules: Rules,
    max_plies: usize,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(writer, "{{")?;
    writeln!(writer, "  \"format\": \"tic-tac-chec-drawing-witness-v1\",")?;
    writeln!(writer, "  \"rulesTag\": \"0x{:08x}\",", rules.stable_tag())?;
    writeln!(
        writer,
        "  \"tablebaseCrc64Xz\": \"0x{:016x}\",",
        tablebase.checksum()
    )?;
    writeln!(writer, "  \"policy\": {},", json_string(POLICY))?;
    writeln!(writer, "  \"maxPlies\": {max_plies},")?;
    writeln!(writer, "  \"initialValue\": \"Draw\",")?;
    writeln!(writer, "  \"prefixPlies\": {},", witness.cycle_start)?;
    writeln!(
        writer,
        "  \"cyclePlies\": {},",
        witness.steps.len() - witness.cycle_start
    )?;
    writeln!(
        writer,
        "  \"repeatedPosition\": {},",
        json_string(&witness.repeated_position.to_string())
    )?;
    writeln!(writer, "  \"steps\": [")?;
    for (index, step) in witness.steps.iter().enumerate() {
        let comma = if index + 1 == witness.steps.len() {
            ""
        } else {
            ","
        };
        writeln!(
            writer,
            "    {{\"ply\":{},\"position\":{},\"sideToMove\":{},\"moveIndex\":{},\"move\":{},\"child\":{},\"legalMoves\":{},\"winningMoves\":{},\"drawingMoves\":{},\"losingMoves\":{}}}{comma}",
            step.ply,
            json_string(&step.position.to_string()),
            json_string(&format!("{:?}", step.side_to_move)),
            step.move_index,
            json_string(&step.action.to_string()),
            json_string(&step.child.to_string()),
            step.legal_moves,
            step.winning_moves,
            step.drawing_moves,
            step.losing_moves,
        )?;
    }
    writeln!(writer, "  ]")?;
    writeln!(writer, "}}")?;
    writer.flush()?;
    Ok(())
}

fn json_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            character if character.is_control() => encoded.push('?'),
            character => encoded.push(character),
        }
    }
    encoded.push('"');
    encoded
}

fn usage() -> ! {
    eprintln!("usage: drawing_witness <tablebase.tb> <output.json> [max-plies]");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tic_tac_chec::probe::Outcome;
    use tic_tac_chec::ranking::OpeningId;

    fn candidate(action: Move, value: Value) -> ProbedMove {
        ProbedMove {
            action,
            child: PositionKey::Opening(OpeningId::new(0).unwrap()),
            outcome: Outcome {
                value,
                distance: None,
            },
            preserves_result: value == Value::Draw,
            optimal: value == Value::Draw,
        }
    }

    #[test]
    fn policy_chooses_least_drawing_action_only() {
        let a1 = tic_tac_chec::Square::new(0, 0).unwrap();
        let b1 = tic_tac_chec::Square::new(1, 0).unwrap();
        let moves = vec![
            candidate(
                Move::Place {
                    piece: tic_tac_chec::PieceKind::Rook,
                    to: a1,
                },
                Value::Draw,
            ),
            candidate(
                Move::Place {
                    piece: tic_tac_chec::PieceKind::Pawn,
                    to: b1,
                },
                Value::Draw,
            ),
            candidate(
                Move::Place {
                    piece: tic_tac_chec::PieceKind::Pawn,
                    to: a1,
                },
                Value::Loss,
            ),
        ];
        let (index, chosen) = choose_drawing_move(&moves).unwrap();
        assert_eq!(index, 1);
        assert_eq!(chosen.action, moves[1].action);
    }

    #[test]
    fn json_strings_escape_control_characters() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
