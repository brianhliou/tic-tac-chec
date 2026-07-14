//! Move-by-move queries over a result-plus-remoteness tablebase.

use std::fmt;

use crate::compact::CompactTablebaseArtifact;
use crate::ranking::{rank_opening, rank_post_opening, OpeningId, PostOpeningId};
use crate::remoteness::DRAW_CODE;
use crate::retrograde::Value;
use crate::tablebase::TablebaseArtifact;
use crate::{Move, Position, Rules};

const UNRESOLVED_CODE: u8 = 254;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PositionKey {
    Opening(OpeningId),
    PostOpening(PostOpeningId),
}

impl fmt::Display for PositionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opening(id) => write!(formatter, "opening:{}", id.get()),
            Self::PostOpening(id) => write!(formatter, "post:{}", id.get()),
        }
    }
}

pub trait TablebaseLookup {
    fn opening_code(&self, id: OpeningId) -> u8;
    fn post_opening_code(&self, id: PostOpeningId) -> u8;
}

impl TablebaseLookup for TablebaseArtifact {
    fn opening_code(&self, id: OpeningId) -> u8 {
        self.opening_codes()[id.get() as usize]
    }

    fn post_opening_code(&self, id: PostOpeningId) -> u8 {
        self.post_codes()[id.get() as usize]
    }
}

impl TablebaseLookup for CompactTablebaseArtifact {
    fn opening_code(&self, id: OpeningId) -> u8 {
        self.opening_code(u64::from(id.get()))
    }

    fn post_opening_code(&self, id: PostOpeningId) -> u8 {
        self.post_code(u64::from(id.get()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub value: Value,
    pub distance: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbedMove {
    pub action: Move,
    pub child: PositionKey,
    /// Result from the perspective of the player choosing `action`.
    pub outcome: Outcome,
    /// Retains the current position's W/L/D result, independent of distance.
    pub preserves_result: bool,
    /// Fastest win, any drawing move, or longest resistance in a loss.
    pub optimal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeResult {
    pub position: PositionKey,
    pub outcome: Outcome,
    pub moves: Vec<ProbedMove>,
}

pub fn probe(
    position: &Position,
    rules: Rules,
    tablebase: &impl TablebaseLookup,
) -> Result<ProbeResult, ProbeError> {
    let key = rank(position)?;
    let outcome = decode(lookup(tablebase, key), key)?;
    let mut moves = Vec::new();
    for action in position.legal_moves(rules) {
        let child_position = position
            .play(action, rules)
            .expect("generated legal move remains playable");
        let child = rank(&child_position)?;
        let child_outcome = decode(lookup(tablebase, child), child)?;
        let mover_outcome = Outcome {
            value: invert(child_outcome.value),
            distance: child_outcome
                .distance
                .map(|distance| {
                    distance
                        .checked_add(1)
                        .ok_or(ProbeError::DistanceOverflow { child })
                })
                .transpose()?,
        };
        let preserves_result = mover_outcome.value == outcome.value;
        let optimal = preserves_result
            && match outcome.value {
                Value::Draw => true,
                Value::Win | Value::Loss => mover_outcome.distance == outcome.distance,
            };
        moves.push(ProbedMove {
            action,
            child,
            outcome: mover_outcome,
            preserves_result,
            optimal,
        });
    }
    if !position.is_terminal() && !moves.iter().any(|candidate| candidate.optimal) {
        return Err(ProbeError::NoOptimalMove { position: key });
    }
    Ok(ProbeResult {
        position: key,
        outcome,
        moves,
    })
}

fn rank(position: &Position) -> Result<PositionKey, ProbeError> {
    if position.opening_complete() {
        rank_post_opening(position)
            .map(PositionKey::PostOpening)
            .ok_or(ProbeError::UnrankablePosition)
    } else {
        rank_opening(position)
            .map(PositionKey::Opening)
            .ok_or(ProbeError::UnrankablePosition)
    }
}

fn lookup(tablebase: &impl TablebaseLookup, key: PositionKey) -> u8 {
    match key {
        PositionKey::Opening(id) => tablebase.opening_code(id),
        PositionKey::PostOpening(id) => tablebase.post_opening_code(id),
    }
}

fn decode(code: u8, position: PositionKey) -> Result<Outcome, ProbeError> {
    match code {
        DRAW_CODE => Ok(Outcome {
            value: Value::Draw,
            distance: None,
        }),
        UNRESOLVED_CODE => Err(ProbeError::UnresolvedCode { position }),
        distance if distance.is_multiple_of(2) => Ok(Outcome {
            value: Value::Loss,
            distance: Some(distance),
        }),
        distance => Ok(Outcome {
            value: Value::Win,
            distance: Some(distance),
        }),
    }
}

fn invert(value: Value) -> Value {
    match value {
        Value::Win => Value::Loss,
        Value::Loss => Value::Win,
        Value::Draw => Value::Draw,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeError {
    UnrankablePosition,
    UnresolvedCode { position: PositionKey },
    NoOptimalMove { position: PositionKey },
    DistanceOverflow { child: PositionKey },
}

impl fmt::Display for ProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProbeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::{self, CompactTablebaseArtifact};
    use crate::ranking::{LOCKED_OPENING_DOMAIN, POST_OPENING_DOMAIN};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct OpeningCodes(Vec<u8>);

    impl TablebaseLookup for OpeningCodes {
        fn opening_code(&self, id: OpeningId) -> u8 {
            self.0[id.get() as usize]
        }

        fn post_opening_code(&self, _: PostOpeningId) -> u8 {
            unreachable!("initial children remain in the opening")
        }
    }

    #[test]
    fn every_drawing_child_is_optimal() {
        let table = OpeningCodes(vec![DRAW_CODE; LOCKED_OPENING_DOMAIN.min(65) as usize]);
        let result = probe(&Position::initial(), Rules::default(), &table).unwrap();
        assert_eq!(result.outcome.value, Value::Draw);
        assert_eq!(result.moves.len(), 64);
        assert!(result.moves.iter().all(|candidate| candidate.optimal));
    }

    #[test]
    fn compact_artifact_drives_the_same_probe_contract() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tic-tac-chec-compact-probe-{}-{nonce}.ttb",
            std::process::id()
        ));
        let codes = vec![DRAW_CODE; 65];
        compact::save_atomic(&path, Rules::default().stable_tag(), &[], &codes).unwrap();
        let table = CompactTablebaseArtifact::load(
            &path,
            Rules::default().stable_tag(),
            0,
            codes.len() as u64,
        )
        .unwrap();
        let result = probe(&Position::initial(), Rules::default(), &table).unwrap();
        assert_eq!(result.outcome.value, Value::Draw);
        assert_eq!(result.moves.len(), 64);
        assert!(result.moves.iter().all(|candidate| candidate.optimal));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn winning_probe_selects_the_shortest_loss_child() {
        let position = Position::initial();
        let first_action = position.legal_moves(Rules::default())[0];
        let first_child = position.play(first_action, Rules::default()).unwrap();
        let winning_child = rank_opening(&first_child).unwrap();
        let mut codes = vec![3; 65];
        codes[0] = 1;
        codes[winning_child.get() as usize] = 0;
        let result = probe(&position, Rules::default(), &OpeningCodes(codes)).unwrap();
        let optimal: Vec<_> = result
            .moves
            .iter()
            .filter(|candidate| candidate.optimal)
            .collect();
        assert_eq!(optimal.len(), 1);
        assert_eq!(optimal[0].action, first_action);
        assert_eq!(optimal[0].outcome.value, Value::Win);
        assert_eq!(optimal[0].outcome.distance, Some(1));
        assert!(!result.moves[1].preserves_result);
    }

    #[test]
    fn losing_probe_selects_the_longest_resistance() {
        let position = Position::initial();
        let first_action = position.legal_moves(Rules::default())[0];
        let first_child = position.play(first_action, Rules::default()).unwrap();
        let delaying_child = rank_opening(&first_child).unwrap();
        let mut codes = vec![1; 65];
        codes[0] = 4;
        codes[delaying_child.get() as usize] = 3;
        let result = probe(&position, Rules::default(), &OpeningCodes(codes)).unwrap();
        assert!(result.moves.iter().all(|candidate| {
            candidate.preserves_result && candidate.outcome.value == Value::Loss
        }));
        let optimal: Vec<_> = result
            .moves
            .iter()
            .filter(|candidate| candidate.optimal)
            .collect();
        assert_eq!(optimal.len(), 1);
        assert_eq!(optimal[0].action, first_action);
        assert_eq!(optimal[0].outcome.distance, Some(4));
    }

    #[test]
    fn published_domains_fit_lookup_keys() {
        assert!(OpeningId::new(LOCKED_OPENING_DOMAIN - 1).is_some());
        assert!(PostOpeningId::new(POST_OPENING_DOMAIN - 1).is_some());
    }
}
