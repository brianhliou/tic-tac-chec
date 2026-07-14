use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use tic_tac_chec::ranking::{LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN};
use tic_tac_chec::remoteness::DRAW_CODE;
use tic_tac_chec::tablebase::TablebaseArtifact;
use tic_tac_chec::Rules;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
enum ResultKind {
    Win = 0,
    Loss = 1,
    Draw = 2,
}

impl ResultKind {
    const ALL: [Self; 3] = [Self::Win, Self::Loss, Self::Draw];

    const fn label(self) -> &'static str {
        match self {
            Self::Win => "Win",
            Self::Loss => "Loss",
            Self::Draw => "Draw",
        }
    }
}

#[derive(Debug)]
struct Comparison {
    nodes: u64,
    matrix: [[u64; 3]; 3],
    first_transition: [[Option<u32>; 3]; 3],
    value_changes: u64,
    exact_code_changes: u64,
    same_value_distance_changes: u64,
    maximum_distance_delta: u8,
    first_maximum_distance_delta: Option<u32>,
}

impl Comparison {
    fn compare(canonical: &[u8], outbound: &[u8]) -> Result<Self, Box<dyn Error>> {
        if canonical.len() != outbound.len() {
            return Err(format!(
                "section length mismatch: {} versus {}",
                canonical.len(),
                outbound.len()
            )
            .into());
        }
        let mut comparison = Self {
            nodes: canonical.len() as u64,
            matrix: [[0; 3]; 3],
            first_transition: [[None; 3]; 3],
            value_changes: 0,
            exact_code_changes: 0,
            same_value_distance_changes: 0,
            maximum_distance_delta: 0,
            first_maximum_distance_delta: None,
        };

        for (raw, (&canonical_code, &outbound_code)) in canonical.iter().zip(outbound).enumerate() {
            let canonical_value = decode(canonical_code)?;
            let outbound_value = decode(outbound_code)?;
            comparison.matrix[canonical_value as usize][outbound_value as usize] += 1;
            comparison.first_transition[canonical_value as usize][outbound_value as usize]
                .get_or_insert(raw as u32);
            comparison.exact_code_changes += u64::from(canonical_code != outbound_code);

            if canonical_value != outbound_value {
                comparison.value_changes += 1;
                continue;
            }
            if canonical_value == ResultKind::Draw || canonical_code == outbound_code {
                continue;
            }
            comparison.same_value_distance_changes += 1;
            let delta = canonical_code.abs_diff(outbound_code);
            if delta > comparison.maximum_distance_delta {
                comparison.maximum_distance_delta = delta;
                comparison.first_maximum_distance_delta = Some(raw as u32);
            }
        }
        Ok(comparison)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let Some(canonical_path) = arguments.get(1) else {
        usage();
    };
    let Some(outbound_path) = arguments.get(2) else {
        usage();
    };
    let Some(output_path) = arguments.get(3) else {
        usage();
    };
    let Some(source_commit) = arguments.get(4) else {
        usage();
    };

    println!("Loading and validating canonical tablebase...");
    let canonical = load(canonical_path, Rules::ORIGINAL_TRAVEL_DIRECTION)?;
    println!("Loading and validating outbound-only tablebase...");
    let outbound = load(outbound_path, Rules::ORIGINAL_OUTBOUND_ONLY)?;
    println!("Comparing every stored position...");
    let post = Comparison::compare(canonical.post_codes(), outbound.post_codes())?;
    let opening = Comparison::compare(canonical.opening_codes(), outbound.opening_codes())?;
    let canonical_initial = decode(canonical.opening_codes()[0])?;
    let outbound_initial = decode(outbound.opening_codes()[0])?;
    write_report(
        Path::new(output_path),
        source_commit,
        &canonical,
        &outbound,
        &post,
        &opening,
        canonical_initial,
        outbound_initial,
    )?;

    println!("post_value_changes = {}", post.value_changes);
    println!("post_exact_code_changes = {}", post.exact_code_changes);
    println!("opening_value_changes = {}", opening.value_changes);
    println!(
        "opening_exact_code_changes = {}",
        opening.exact_code_changes
    );
    println!("canonical_initial = {}", canonical_initial.label());
    println!("outbound_initial = {}", outbound_initial.label());
    println!("output = {output_path}");
    Ok(())
}

fn load(path: &str, rules: Rules) -> Result<TablebaseArtifact, Box<dyn Error>> {
    Ok(TablebaseArtifact::load(
        path,
        rules.stable_tag(),
        POST_OPENING_DOMAIN as u64,
        LOCKED_OPENING_DOMAIN as u64,
    )?)
}

#[allow(clippy::too_many_arguments)]
fn write_report(
    path: &Path,
    source_commit: &str,
    canonical: &TablebaseArtifact,
    outbound: &TablebaseArtifact,
    post: &Comparison,
    opening: &Comparison,
    canonical_initial: ResultKind,
    outbound_initial: ResultKind,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(writer, "# Returning-pawn variant comparison")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "Exact position-by-position comparison of the original-edition travel-direction and outbound-only result-plus-remoteness tablebases."
    )?;
    writeln!(writer)?;
    writeln!(writer, "- Comparator source commit: `{source_commit}`")?;
    writeln!(
        writer,
        "- Canonical travel-direction tag: `0x{:08x}`; CRC-64/XZ: `0x{:016x}`",
        canonical.rules_tag(),
        canonical.checksum()
    )?;
    writeln!(
        writer,
        "- Outbound-only tag: `0x{:08x}`; CRC-64/XZ: `0x{:016x}`",
        outbound.rules_tag(),
        outbound.checksum()
    )?;
    writeln!(
        writer,
        "- Initial value: {} under travel-direction, {} under outbound-only",
        canonical_initial.label(),
        outbound_initial.label()
    )?;
    write_section(&mut writer, "Post-opening", "post", post)?;
    write_section(&mut writer, "Locked opening", "opening", opening)?;
    writeln!(writer)?;
    writeln!(writer, "## Interpretation")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "The rule ambiguity does not change the perfect-play value of the empty board, but it changes values and decisive distances throughout the table. Matrix rows are the canonical travel-direction result; columns are the outbound-only result. IDs are the lowest dense representative for each nonempty transition, not claims about reachability from the empty board or strategic frequency. Both artifacts were solved and audited independently before this bytewise comparison."
    )?;
    writer.flush()?;
    Ok(())
}

fn write_section(
    writer: &mut impl Write,
    label: &str,
    id_prefix: &str,
    comparison: &Comparison,
) -> std::io::Result<()> {
    writeln!(writer)?;
    writeln!(writer, "## {label}")?;
    writeln!(writer)?;
    writeln!(writer, "| Travel ↓ / outbound → | Win | Loss | Draw |")?;
    writeln!(writer, "| --- | ---: | ---: | ---: |")?;
    for canonical in ResultKind::ALL {
        writeln!(
            writer,
            "| {} | {} | {} | {} |",
            canonical.label(),
            commas(comparison.matrix[canonical as usize][ResultKind::Win as usize]),
            commas(comparison.matrix[canonical as usize][ResultKind::Loss as usize]),
            commas(comparison.matrix[canonical as usize][ResultKind::Draw as usize]),
        )?;
    }
    writeln!(writer)?;
    writeln!(writer, "- Positions: {}", commas(comparison.nodes))?;
    writeln!(
        writer,
        "- W/L/D value changes: {} ({:.6}%)",
        commas(comparison.value_changes),
        percentage(comparison.value_changes, comparison.nodes)
    )?;
    writeln!(
        writer,
        "- Exact result-or-distance code changes: {} ({:.6}%)",
        commas(comparison.exact_code_changes),
        percentage(comparison.exact_code_changes, comparison.nodes)
    )?;
    writeln!(
        writer,
        "- Same-result decisive positions with a changed distance: {}",
        commas(comparison.same_value_distance_changes)
    )?;
    if let Some(raw) = comparison.first_maximum_distance_delta {
        writeln!(
            writer,
            "- Maximum same-result distance change: {} plies (first at `{id_prefix}:{raw}`)",
            comparison.maximum_distance_delta
        )?;
    }

    let transitions: Vec<_> = ResultKind::ALL
        .into_iter()
        .flat_map(|from| ResultKind::ALL.into_iter().map(move |to| (from, to)))
        .filter(|(from, to)| from != to)
        .filter_map(|(from, to)| {
            comparison.first_transition[from as usize][to as usize]
                .map(|raw| (from, to, raw, comparison.matrix[from as usize][to as usize]))
        })
        .collect();
    if !transitions.is_empty() {
        writeln!(writer, "- First dense representatives:")?;
        for (from, to, raw, count) in transitions {
            writeln!(
                writer,
                "  - {} → {}: `{id_prefix}:{raw}` ({} positions)",
                from.label(),
                to.label(),
                commas(count)
            )?;
        }
    }
    Ok(())
}

fn decode(code: u8) -> Result<ResultKind, Box<dyn Error>> {
    match code {
        DRAW_CODE => Ok(ResultKind::Draw),
        254 => Err("tablebase contains reserved code 254".into()),
        distance if distance.is_multiple_of(2) => Ok(ResultKind::Loss),
        _ => Ok(ResultKind::Win),
    }
}

fn percentage(part: u64, whole: u64) -> f64 {
    part as f64 * 100.0 / whole as f64
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
    eprintln!("usage: variant_compare <travel.tb> <outbound.tb> <output.md> <source-commit>");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_counts_value_and_distance_changes_separately() {
        let canonical = [1, 2, 3, DRAW_CODE, DRAW_CODE, 4];
        let outbound = [3, 2, DRAW_CODE, 1, DRAW_CODE, 6];
        let comparison = Comparison::compare(&canonical, &outbound).unwrap();
        assert_eq!(comparison.nodes, 6);
        assert_eq!(comparison.value_changes, 2);
        assert_eq!(comparison.exact_code_changes, 4);
        assert_eq!(comparison.same_value_distance_changes, 2);
        assert_eq!(comparison.maximum_distance_delta, 2);
        assert_eq!(comparison.first_maximum_distance_delta, Some(0));
        assert_eq!(comparison.matrix[ResultKind::Win as usize], [1, 0, 1]);
        assert_eq!(comparison.matrix[ResultKind::Loss as usize], [0, 2, 0]);
        assert_eq!(comparison.matrix[ResultKind::Draw as usize], [1, 0, 1]);
    }

    #[test]
    fn comparison_rejects_different_section_lengths() {
        assert!(Comparison::compare(&[DRAW_CODE], &[]).is_err());
    }
}
