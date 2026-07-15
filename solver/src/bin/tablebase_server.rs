use std::error::Error;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tic_tac_chec::compact::CompactTablebaseArtifact;
use tic_tac_chec::probe::probe;
use tic_tac_chec::ranking::{LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN};
use tic_tac_chec::{
    Color, Move, PawnDirection, Piece, PieceKind, Position, ReturningPawnCapture, Rules, Square,
    BOARD_CELLS,
};

const INDEX_HTML: &str = include_str!("../../web/index.html");
const WRITE_UP_HTML: &str = include_str!("../../web/write-up.html");
const WHITE_PAWN: &str = include_str!("../../web/pieces/cburnett/wP.svg");
const WHITE_KNIGHT: &str = include_str!("../../web/pieces/cburnett/wN.svg");
const WHITE_BISHOP: &str = include_str!("../../web/pieces/cburnett/wB.svg");
const WHITE_ROOK: &str = include_str!("../../web/pieces/cburnett/wR.svg");
const BLACK_PAWN: &str = include_str!("../../web/pieces/cburnett/bP.svg");
const BLACK_KNIGHT: &str = include_str!("../../web/pieces/cburnett/bN.svg");
const BLACK_BISHOP: &str = include_str!("../../web/pieces/cburnett/bB.svg");
const BLACK_ROOK: &str = include_str!("../../web/pieces/cburnett/bR.svg");
const DEFAULT_PORT: u16 = 4173;
const DEFAULT_WORKERS: usize = 8;
const MAX_PATH_PLIES: usize = 512;
const MAX_REQUEST_HEAD: usize = 16 * 1024;
const MAX_REQUESTS_PER_CONNECTION: usize = 100;
const CONNECTION_QUEUE: usize = 128;
const IO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
struct RequestHead {
    method: String,
    target: String,
    keep_alive: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(path) = arguments.get(1) else {
        usage();
    };
    let positional_port = arguments
        .get(2)
        .filter(|argument| !argument.starts_with("--"))
        .map(|argument| argument.parse::<u16>())
        .transpose()?;
    let port = match positional_port {
        Some(port) => port,
        None => std::env::var("PORT")
            .ok()
            .map(|value| value.parse::<u16>())
            .transpose()?
            .unwrap_or(DEFAULT_PORT),
    };
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    let workers = std::env::var("TABLEBASE_WORKERS")
        .ok()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(DEFAULT_WORKERS);
    if workers == 0 {
        return Err("TABLEBASE_WORKERS must be positive".into());
    }
    let rules = rules(&arguments);
    println!("Loading and validating tablebase...");
    let tablebase = Arc::new(CompactTablebaseArtifact::load(
        Path::new(path),
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?);
    let address = format!("{host}:{port}");
    let listener = TcpListener::bind(&address)?;
    let (sender, receiver) = mpsc::sync_channel::<TcpStream>(CONNECTION_QUEUE);
    let receiver = Arc::new(Mutex::new(receiver));
    for worker in 0..workers {
        let receiver = Arc::clone(&receiver);
        let tablebase = Arc::clone(&tablebase);
        std::thread::Builder::new()
            .name(format!("tablebase-worker-{worker}"))
            .spawn(move || worker_loop(&receiver, &tablebase, rules))?;
    }
    println!("Tic Tac Chec tablebase: http://{address}");
    println!("workers = {workers}");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if sender.send(stream).is_err() {
                    return Err("tablebase worker queue disconnected".into());
                }
            }
            Err(error) => eprintln!("connection error: {error}"),
        }
    }
    Ok(())
}

fn worker_loop(
    receiver: &Mutex<mpsc::Receiver<TcpStream>>,
    tablebase: &CompactTablebaseArtifact,
    rules: Rules,
) {
    loop {
        let stream = receiver.lock().expect("worker queue poisoned").recv();
        let Ok(stream) = stream else {
            break;
        };
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        if let Err(error) = handle_connection(stream, tablebase, rules) {
            eprintln!("request error: {error}");
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    tablebase: &CompactTablebaseArtifact,
    rules: Rules,
) -> Result<(), Box<dyn Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    for request_index in 0..MAX_REQUESTS_PER_CONNECTION {
        let request = match read_request(&mut reader) {
            Ok(Some(request)) => request,
            Ok(None) => return Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        let keep_alive = request.keep_alive && request_index + 1 < MAX_REQUESTS_PER_CONNECTION;
        handle(
            &mut stream,
            &request.method,
            &request.target,
            tablebase,
            rules,
            keep_alive,
        )?;
        if !keep_alive {
            return Ok(());
        }
    }
    Ok(())
}

fn read_request(reader: &mut impl BufRead) -> io::Result<Option<RequestHead>> {
    let mut line = String::new();
    let mut bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        return Ok(None);
    }
    if bytes > MAX_REQUEST_HEAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request headers are too large",
        ));
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let version = parts.next().unwrap_or_default();
    if method.is_empty() || target.is_empty() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "malformed HTTP request line",
        ));
    }
    let mut keep_alive = version == "HTTP/1.1";
    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request ended before its headers",
            ));
        }
        bytes += read;
        if bytes > MAX_REQUEST_HEAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request headers are too large",
            ));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("connection") {
            for directive in value.split(',').map(str::trim) {
                if directive.eq_ignore_ascii_case("close") {
                    keep_alive = false;
                } else if directive.eq_ignore_ascii_case("keep-alive") {
                    keep_alive = true;
                }
            }
        }
    }
    Ok(Some(RequestHead {
        method,
        target,
        keep_alive,
    }))
}

fn handle(
    stream: &mut TcpStream,
    method: &str,
    target: &str,
    tablebase: &CompactTablebaseArtifact,
    rules: Rules,
    keep_alive: bool,
) -> Result<(), Box<dyn Error>> {
    if method != "GET" {
        return respond(
            stream,
            405,
            "text/plain; charset=utf-8",
            "Method not allowed",
            keep_alive,
        );
    }
    if let Some(page) = page_asset(target) {
        return respond_cached(
            stream,
            200,
            "text/html; charset=utf-8",
            page,
            "no-cache",
            keep_alive,
        );
    }
    if target == "/health" {
        return respond(
            stream,
            200,
            "application/json",
            "{\"status\":\"ok\"}",
            keep_alive,
        );
    }
    if let Some(asset) = piece_asset(target) {
        return respond_cached(
            stream,
            200,
            "image/svg+xml",
            asset,
            "public, max-age=31536000, immutable",
            keep_alive,
        );
    }
    if target == "/api/probe" || target.starts_with("/api/probe?") {
        let body = match parse_path(target).and_then(|path| probe_json(&path, tablebase, rules)) {
            Ok(body) => body,
            Err(error) => {
                let body = format!("{{\"error\":{}}}", json_string(&error));
                return respond(stream, 400, "application/json", &body, keep_alive);
            }
        };
        return respond(stream, 200, "application/json", &body, keep_alive);
    }
    respond(
        stream,
        404,
        "text/plain; charset=utf-8",
        "Not found",
        keep_alive,
    )
}

fn page_asset(target: &str) -> Option<&'static str> {
    match target {
        "/" => Some(INDEX_HTML),
        "/write-up" | "/write-up/" => Some(WRITE_UP_HTML),
        _ => None,
    }
}

fn piece_asset(target: &str) -> Option<&'static str> {
    match target {
        "/pieces/cburnett/wP.svg" => Some(WHITE_PAWN),
        "/pieces/cburnett/wN.svg" => Some(WHITE_KNIGHT),
        "/pieces/cburnett/wB.svg" => Some(WHITE_BISHOP),
        "/pieces/cburnett/wR.svg" => Some(WHITE_ROOK),
        "/pieces/cburnett/bP.svg" => Some(BLACK_PAWN),
        "/pieces/cburnett/bN.svg" => Some(BLACK_KNIGHT),
        "/pieces/cburnett/bB.svg" => Some(BLACK_BISHOP),
        "/pieces/cburnett/bR.svg" => Some(BLACK_ROOK),
        _ => None,
    }
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
    let decoded = decode_query_component(encoded)?;
    let path: Vec<_> = decoded
        .split(',')
        .map(|index| {
            index
                .parse::<usize>()
                .map_err(|_| "path must contain comma-separated move indexes".to_owned())
        })
        .collect::<Result<_, _>>()?;
    if path.len() > MAX_PATH_PLIES {
        return Err(format!("path may contain at most {MAX_PATH_PLIES} plies"));
    }
    Ok(path)
}

fn decode_query_component(encoded: &str) -> Result<String, String> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = bytes.get(index + 1).and_then(|byte| hex_value(*byte));
                let low = bytes.get(index + 2).and_then(|byte| hex_value(*byte));
                let (Some(high), Some(low)) = (high, low) else {
                    return Err("path contains invalid percent encoding".to_owned());
                };
                decoded.push((high << 4) | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| "path must be valid UTF-8".to_owned())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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
    tablebase: &CompactTablebaseArtifact,
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
    json.push_str("],\"hands\":{");
    for (color_index, color) in Color::ALL.into_iter().enumerate() {
        if color_index != 0 {
            json.push(',');
        }
        json.push_str(&json_string(&format!("{color:?}")));
        json.push(':');
        json.push('[');
        let mut first = true;
        for kind in PieceKind::ALL {
            if position.piece_square(Piece { color, kind }).is_none() {
                if !first {
                    json.push(',');
                }
                json.push_str(&json_string(&format!("{kind:?}")));
                first = false;
            }
        }
        json.push(']');
    }
    json.push_str("},\"history\":[");
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
    keep_alive: bool,
) -> Result<(), Box<dyn Error>> {
    respond_cached(stream, status, content_type, body, "no-store", keep_alive)
}

fn respond_cached(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
    cache_control: &str,
    keep_alive: bool,
) -> Result<(), Box<dyn Error>> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let connection = if keep_alive { "keep-alive" } else { "close" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: {cache_control}\r\nX-Content-Type-Options: nosniff\r\nConnection: {connection}\r\nKeep-Alive: timeout={}, max={}\r\n\r\n{body}",
        body.len(),
        IO_TIMEOUT.as_secs(),
        MAX_REQUESTS_PER_CONNECTION
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
    eprintln!("usage: tablebase_server <compact.ttb> [port] [--pawn=travel|outbound|opponent]");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_parser_handles_empty_single_and_multiple_plies() {
        assert_eq!(parse_path("/api/probe").unwrap(), Vec::<usize>::new());
        assert_eq!(parse_path("/api/probe?path=").unwrap(), Vec::<usize>::new());
        assert_eq!(parse_path("/api/probe?path=42").unwrap(), vec![42]);
        assert_eq!(
            parse_path("/api/probe?path=0,17,4").unwrap(),
            vec![0, 17, 4]
        );
        assert_eq!(
            parse_path("/api/probe?path=0%2C17%2c4").unwrap(),
            vec![0, 17, 4]
        );
    }

    #[test]
    fn path_parser_rejects_invalid_percent_encoding() {
        assert_eq!(
            parse_path("/api/probe?path=0%2").unwrap_err(),
            "path contains invalid percent encoding"
        );
    }

    #[test]
    fn request_parser_handles_persistent_and_pipelined_requests() {
        let requests = b"GET /health HTTP/1.1\r\nHost: example\r\n\r\nGET / HTTP/1.1\r\nConnection: close\r\n\r\n";
        let mut reader = BufReader::new(&requests[..]);
        assert_eq!(
            read_request(&mut reader).unwrap(),
            Some(RequestHead {
                method: "GET".to_owned(),
                target: "/health".to_owned(),
                keep_alive: true,
            })
        );
        assert_eq!(
            read_request(&mut reader).unwrap(),
            Some(RequestHead {
                method: "GET".to_owned(),
                target: "/".to_owned(),
                keep_alive: false,
            })
        );
        assert_eq!(read_request(&mut reader).unwrap(), None);
    }

    #[test]
    fn request_parser_respects_http_10_keep_alive() {
        let request = b"GET /health HTTP/1.0\r\nConnection: keep-alive\r\n\r\n";
        let mut reader = BufReader::new(&request[..]);
        assert!(read_request(&mut reader).unwrap().unwrap().keep_alive);
    }

    #[test]
    fn path_parser_rejects_unbounded_history() {
        let target = format!(
            "/api/probe?path={}",
            std::iter::repeat_n("0", MAX_PATH_PLIES + 1)
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(parse_path(&target).unwrap_err().contains("at most"));
    }

    #[test]
    fn static_pages_have_stable_routes() {
        assert!(page_asset("/")
            .unwrap()
            .contains("Interactive Tablebase Explorer"));
        assert!(page_asset("/write-up")
            .unwrap()
            .contains("Solving Tic Tac Chec"));
        assert_eq!(page_asset("/write-up"), page_asset("/write-up/"));
        assert!(page_asset("/missing").is_none());
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
