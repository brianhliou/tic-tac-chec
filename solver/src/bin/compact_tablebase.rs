use std::error::Error;
use std::path::Path;
use std::time::Instant;

use tic_tac_chec::compact::{self, CompactTablebaseArtifact};
use tic_tac_chec::ranking::{LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN};
use tic_tac_chec::tablebase::TablebaseArtifact;
use tic_tac_chec::{ReturningPawnCapture, Rules};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(command) = arguments.get(1).map(String::as_str) else {
        usage();
    };
    let Some(path) = arguments.get(2) else {
        usage();
    };
    let rules = rules(&arguments);
    match command {
        "pack" => {
            let Some(output) = arguments
                .get(3)
                .filter(|argument| !argument.starts_with("--"))
            else {
                usage();
            };
            pack(rules, path, output)
        }
        "verify" => verify(rules, path),
        "compare" => {
            let Some(compact) = arguments
                .get(3)
                .filter(|argument| !argument.starts_with("--"))
            else {
                usage();
            };
            compare(rules, path, compact)
        }
        _ => usage(),
    }
}

fn pack(
    rules: Rules,
    source_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    println!("phase = compact-pack");
    println!("rules_tag = {:#010x}", rules.stable_tag());
    let load_start = Instant::now();
    let source = load_source(rules, source_path)?;
    println!(
        "source_load_seconds = {:.6}",
        load_start.elapsed().as_secs_f64()
    );

    let pack_start = Instant::now();
    let checksum = compact::save_atomic(
        &output_path,
        rules.stable_tag(),
        source.post_codes(),
        source.opening_codes(),
    )?;
    println!("compact_crc64 = {checksum:#018x}");
    println!("pack_seconds = {:.6}", pack_start.elapsed().as_secs_f64());

    let verify_start = Instant::now();
    let compact = load_compact(rules, output_path)?;
    compare_loaded(&source, &compact)?;
    println!("post_nodes_compared = {POST_OPENING_DOMAIN}");
    println!("opening_nodes_compared = {LOCKED_OPENING_DOMAIN}");
    println!("post_decisive = {}", compact.post_decisive());
    println!("opening_decisive = {}", compact.opening_decisive());
    println!(
        "compare_seconds = {:.6}",
        verify_start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn verify(rules: Rules, path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    println!("phase = compact-verify");
    let start = Instant::now();
    let compact = load_compact(rules, path)?;
    println!("rules_tag = {:#010x}", compact.rules_tag());
    println!("crc64 = {:#018x}", compact.checksum());
    println!("post_nodes = {}", compact.post_nodes());
    println!("opening_nodes = {}", compact.opening_nodes());
    println!("post_decisive = {}", compact.post_decisive());
    println!("opening_decisive = {}", compact.opening_decisive());
    println!(
        "verification_seconds = {:.6}",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn compare(
    rules: Rules,
    source_path: impl AsRef<Path>,
    compact_path: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    println!("phase = compact-compare");
    let start = Instant::now();
    let source = load_source(rules, source_path)?;
    let compact = load_compact(rules, compact_path)?;
    compare_loaded(&source, &compact)?;
    println!("post_nodes_compared = {POST_OPENING_DOMAIN}");
    println!("opening_nodes_compared = {LOCKED_OPENING_DOMAIN}");
    println!("comparison_seconds = {:.6}", start.elapsed().as_secs_f64());
    Ok(())
}

fn load_source(rules: Rules, path: impl AsRef<Path>) -> Result<TablebaseArtifact, Box<dyn Error>> {
    Ok(TablebaseArtifact::load(
        path,
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?)
}

fn load_compact(
    rules: Rules,
    path: impl AsRef<Path>,
) -> Result<CompactTablebaseArtifact, Box<dyn Error>> {
    Ok(CompactTablebaseArtifact::load(
        path,
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?)
}

fn compare_loaded(
    source: &TablebaseArtifact,
    compact: &CompactTablebaseArtifact,
) -> Result<(), Box<dyn Error>> {
    for (index, &expected) in source.post_codes().iter().enumerate() {
        let actual = compact.post_code(index as u64);
        if actual != expected {
            return Err(format!(
                "post-opening mismatch at {index}: source={expected}, compact={actual}"
            )
            .into());
        }
    }
    for (index, &expected) in source.opening_codes().iter().enumerate() {
        let actual = compact.opening_code(index as u64);
        if actual != expected {
            return Err(format!(
                "opening mismatch at {index}: source={expected}, compact={actual}"
            )
            .into());
        }
    }
    Ok(())
}

fn rules(arguments: &[String]) -> Rules {
    let capture = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--pawn="))
        .map(|name| match name {
            "travel" => ReturningPawnCapture::TravelDirection,
            "outbound" => ReturningPawnCapture::OutboundOnly,
            "opponent" => ReturningPawnCapture::TowardOpponent,
            _ => usage(),
        })
        .unwrap_or(ReturningPawnCapture::TravelDirection);
    Rules {
        returning_pawn_capture: capture,
    }
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  compact_tablebase pack <source.tb> <compact.ttb> [--pawn=travel|outbound|opponent]\n  compact_tablebase verify <compact.ttb> [--pawn=travel|outbound|opponent]\n  compact_tablebase compare <source.tb> <compact.ttb> [--pawn=travel|outbound|opponent]"
    );
    std::process::exit(2);
}
