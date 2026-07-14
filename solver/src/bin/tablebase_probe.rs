use std::error::Error;
use std::path::Path;

use tic_tac_chec::probe::probe;
use tic_tac_chec::ranking::{
    unrank_opening, unrank_post_opening, OpeningId, PostOpeningId, LOCKED_OPENING_DOMAIN,
    POST_OPENING_DOMAIN,
};
use tic_tac_chec::tablebase::TablebaseArtifact;
use tic_tac_chec::{Color, PieceKind, Position, ReturningPawnCapture, Rules, Square, BOARD_SIDE};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(path) = arguments.get(1) else {
        usage();
    };
    let Some(phase) = arguments.get(2) else {
        usage();
    };
    let Some(raw) = arguments.get(3).and_then(|raw| raw.parse::<u32>().ok()) else {
        usage();
    };
    let rules = rules(&arguments);
    let position = position(phase, raw)?;
    let tablebase = TablebaseArtifact::load(
        Path::new(path),
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?;
    let result = probe(&position, rules, &tablebase)?;
    print_board(&position);
    println!("position = {}", result.position);
    println!("side_to_move = {:?}", position.side_to_move());
    println!("value = {:?}", result.outcome.value);
    print_distance("distance", result.outcome.distance);
    println!("legal_moves = {}", result.moves.len());
    println!(
        "optimal_moves = {}",
        result
            .moves
            .iter()
            .filter(|candidate| candidate.optimal)
            .count()
    );
    for candidate in result.moves {
        print!(
            "move = {} child = {} result = {:?} ",
            candidate.action, candidate.child, candidate.outcome.value
        );
        match candidate.outcome.distance {
            Some(distance) => print!("distance = {distance} "),
            None => print!("distance = draw "),
        }
        println!(
            "preserves_result = {} optimal = {}",
            candidate.preserves_result, candidate.optimal
        );
    }
    Ok(())
}

fn position(phase: &str, raw: u32) -> Result<Position, Box<dyn Error>> {
    match phase {
        "opening" => OpeningId::new(raw)
            .map(unrank_opening)
            .ok_or_else(|| format!("opening ID must be below {LOCKED_OPENING_DOMAIN}").into()),
        "post" => PostOpeningId::new(raw)
            .map(unrank_post_opening)
            .ok_or_else(|| format!("post ID must be below {POST_OPENING_DOMAIN}").into()),
        _ => Err("phase must be 'opening' or 'post'".into()),
    }
}

fn print_board(position: &Position) {
    for rank in (0..BOARD_SIDE).rev() {
        print!("{} ", rank + 1);
        for file in 0..BOARD_SIDE {
            let square = Square::new(file, rank).unwrap();
            let symbol = position.at(square).map_or('.', |piece| {
                let symbol = match piece.kind {
                    PieceKind::Pawn => 'p',
                    PieceKind::Knight => 'n',
                    PieceKind::Bishop => 'b',
                    PieceKind::Rook => 'r',
                };
                if piece.color == Color::White {
                    symbol.to_ascii_uppercase()
                } else {
                    symbol
                }
            });
            print!("{symbol} ");
        }
        println!();
    }
    println!("  a b c d");
}

fn print_distance(label: &str, distance: Option<u8>) {
    match distance {
        Some(distance) => println!("{label} = {distance}"),
        None => println!("{label} = draw"),
    }
}

fn rules(arguments: &[String]) -> Rules {
    let returning_pawn_capture = match arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--pawn="))
    {
        None | Some("travel") => ReturningPawnCapture::TravelDirection,
        Some("outbound") => ReturningPawnCapture::OutboundOnly,
        Some("opponent") => ReturningPawnCapture::TowardOpponent,
        Some(other) => panic!("unknown pawn variant: {other}"),
    };
    Rules {
        returning_pawn_capture,
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: tablebase_probe <tablebase.tb> <opening|post> <id> [--pawn=travel|outbound|opponent]"
    );
    std::process::exit(2)
}
