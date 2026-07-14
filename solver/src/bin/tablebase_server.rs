use std::error::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

use tic_tac_chec::probe::probe;
use tic_tac_chec::ranking::{LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN};
use tic_tac_chec::tablebase::TablebaseArtifact;
use tic_tac_chec::{
    Move, PawnDirection, PieceKind, Position, ReturningPawnCapture, Rules, Square, BOARD_CELLS,
};

const INDEX_HTML: &str = include_str!("../../web/index.html");

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(path) = arguments.get(1) else {
        usage();
    };
    let port = arguments
        .get(2)
        .filter(|argument| !argument.starts_with("--"))
        .map(|argument| argument.parse::<u16>())
        .transpose()?
        .unwrap_or(4173);
    let rules = rules(&arguments);
    println!("Loading and validating tablebase...");
    let tablebase = TablebaseArtifact::load(
        Path::new(path),
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?;
    let address = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&address)?;
    println!("Tic Tac Chec tablebase: http://{address}");
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle(&mut stream, &tablebase, rules) {
                    eprintln!("request error: {error}");
                }
            }
            Err(error) => eprintln!("connection error: {error}"),
        }
    }
    Ok(())
}

fn handle(
    stream: &mut TcpStream,
    tablebase: &TablebaseArtifact,
    rules: Rules,
) -> Result<(), Box<dyn Error>> {
    let mut request = [0_u8; 16 * 1024];
    let length = stream.read(&mut request)?;
    let request = std::str::from_utf8(&request[..length])?;
    let Some(line) = request.lines().next() else {
        return Ok(());
    };
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" {
        return respond(
            stream,
            405,
            "text/plain; charset=utf-8",
            "Method not allowed",
        );
    }
    if target == "/" {
        return respond(stream, 200, "text/html; charset=utf-8", INDEX_HTML);
    }
    if target == "/health" {
        return respond(stream, 200, "application/json", "{\"status\":\"ok\"}");
    }
    if target.starts_with("/api/probe") {
        let body = match parse_path(target).and_then(|path| probe_json(&path, tablebase, rules)) {
            Ok(body) => body,
            Err(error) => {
                let body = format!("{{\"error\":{}}}", json_string(&error));
                return respond(stream, 400, "application/json", &body);
            }
        };
        return respond(stream, 200, "application/json", &body);
    }
    respond(stream, 404, "text/plain; charset=utf-8", "Not found")
}

fn parse_path(target: &str) -> Result<Vec<usize>, String> {
    let Some(query) = target.split_once('?').map(|(_, query)| query) else {
        return Ok(Vec::new());
    };
    let encoded = query
        .split('&')
        .find_map(|part| part.strip_prefix("path="))
        .unwrap_or_default();
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    encoded
        .split(',')
        .map(|index| {
            index
                .parse::<usize>()
                .map_err(|_| "path must contain comma-separated move indexes".to_owned())
        })
        .collect()
}

fn replay(path: &[usize], rules: Rules) -> Result<(Position, Vec<Move>), String> {
    let mut position = Position::initial();
    let mut history = Vec::with_capacity(path.len());
    for (ply, &index) in path.iter().enumerate() {
        let moves = position.legal_moves(rules);
        let Some(&action) = moves.get(index) else {
            return Err(format!(
                "move index {index} is out of range at ply {ply} ({} legal moves)",
                moves.len()
            ));
        };
        position = position
            .play(action, rules)
            .expect("indexed generated move remains legal");
        history.push(action);
    }
    Ok((position, history))
}

fn probe_json(
    path: &[usize],
    tablebase: &TablebaseArtifact,
    rules: Rules,
) -> Result<String, String> {
    let (position, history) = replay(path, rules)?;
    let result = probe(&position, rules, tablebase).map_err(|error| error.to_string())?;
    let mut json = String::with_capacity(16 * 1024);
    json.push_str("{\"position\":");
    json.push_str(&json_string(&result.position.to_string()));
    json.push_str(",\"sideToMove\":");
    json.push_str(json_string(&format!("{:?}", position.side_to_move())).as_str());
    json.push_str(",\"value\":");
    json.push_str(json_string(&format!("{:?}", result.outcome.value)).as_str());
    json.push_str(",\"distance\":");
    push_distance(&mut json, result.outcome.distance);
    json.push_str(",\"board\":[");
    for index in 0..BOARD_CELLS as u8 {
        if index != 0 {
            json.push(',');
        }
        let square = Square::from_index(index).unwrap();
        match position.at(square) {
            None => json.push_str("null"),
            Some(piece) => {
                let symbol = match piece.kind {
                    PieceKind::Pawn => 'P',
                    PieceKind::Knight => 'N',
                    PieceKind::Bishop => 'B',
                    PieceKind::Rook => 'R',
                };
                json.push_str("{\"color\":");
                json.push_str(json_string(&format!("{:?}", piece.color)).as_str());
                json.push_str(",\"piece\":");
                json.push_str(json_string(&format!("{:?}", piece.kind)).as_str());
                json.push_str(",\"symbol\":");
                json.push_str(&json_string(&symbol.to_string()));
                if piece.kind == PieceKind::Pawn {
                    let direction = match position.pawn_direction(piece.color) {
                        PawnDirection::TowardBlack => "up",
                        PawnDirection::TowardWhite => "down",
                    };
                    json.push_str(",\"direction\":");
                    json.push_str(&json_string(direction));
                }
                json.push('}');
            }
        }
    }
    json.push_str("],\"history\":[");
    for (index, action) in history.iter().enumerate() {
        if index != 0 {
            json.push(',');
        }
        json.push_str(&json_string(&action.to_string()));
    }
    json.push_str("],\"moves\":[");
    for (index, candidate) in result.moves.iter().enumerate() {
        if index != 0 {
            json.push(',');
        }
        json.push_str("{\"index\":");
        json.push_str(&index.to_string());
        json.push_str(",\"notation\":");
        json.push_str(&json_string(&candidate.action.to_string()));
        json.push_str(",\"child\":");
        json.push_str(&json_string(&candidate.child.to_string()));
        json.push_str(",\"result\":");
        json.push_str(&json_string(&format!("{:?}", candidate.outcome.value)));
        json.push_str(",\"distance\":");
        push_distance(&mut json, candidate.outcome.distance);
        json.push_str(",\"preservesResult\":");
        json.push_str(if candidate.preserves_result {
            "true"
        } else {
            "false"
        });
        json.push_str(",\"optimal\":");
        json.push_str(if candidate.optimal { "true" } else { "false" });
        json.push('}');
    }
    json.push_str("]}");
    Ok(json)
}

fn push_distance(json: &mut String, distance: Option<u8>) {
    match distance {
        Some(distance) => json.push_str(&distance.to_string()),
        None => json.push_str("null"),
    }
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

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> Result<(), Box<dyn Error>> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()?;
    Ok(())
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
    eprintln!("usage: tablebase_server <tablebase.tb> [port] [--pawn=travel|outbound|opponent]");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tic_tac_chec::Color;

    #[test]
    fn path_parser_handles_empty_and_multiple_plies() {
        assert_eq!(parse_path("/api/probe").unwrap(), Vec::<usize>::new());
        assert_eq!(parse_path("/api/probe?path=").unwrap(), Vec::<usize>::new());
        assert_eq!(
            parse_path("/api/probe?path=0,17,4").unwrap(),
            vec![0, 17, 4]
        );
    }

    #[test]
    fn replay_preserves_absolute_board_orientation() {
        let (position, history) = replay(&[0], Rules::default()).unwrap();
        assert_eq!(history[0].to_string(), "P@a1");
        assert_eq!(position.side_to_move(), Color::Black);
        assert_eq!(
            position.at(Square::new(0, 0).unwrap()).unwrap().color,
            Color::White
        );
    }
}
