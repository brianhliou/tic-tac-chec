//! Independent detectors for one-move alignment wins.
//!
//! The direct detector reasons from the ten winning lines and piece geometry.
//! The reference detector changes the side to move, runs the production move
//! generator, and checks the resulting positions. Their agreement is the main
//! correctness guardrail for the exhaustive threat census.

use crate::{
    graph::for_each_legal_move, initial_pawn_direction, Color, Move, Piece, PieceKind, Position,
    ReturningPawnCapture, Rules, Square, BOARD_CELLS, WIN_LINES,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImmediateWins {
    pub actions: u8,
    pub drops: u8,
    pub moves_to_empty: u8,
    pub captures: u8,
    action_set: [u64; 5],
}

impl ImmediateWins {
    pub const fn is_live(self) -> bool {
        self.actions != 0
    }

    fn add(&mut self, position: &Position, action: Move) {
        let encoded = match action {
            Move::Place { piece, to } => piece as usize * BOARD_CELLS + to.index(),
            Move::Move { from, to } => {
                PieceKind::ALL.len() * BOARD_CELLS + from.index() * BOARD_CELLS + to.index()
            }
        };
        let word = encoded / u64::BITS as usize;
        let bit = 1_u64 << (encoded % u64::BITS as usize);
        if self.action_set[word] & bit != 0 {
            return;
        }
        self.action_set[word] |= bit;
        self.actions += 1;
        match action {
            Move::Place { .. } => self.drops += 1,
            Move::Move { to, .. } if position.at(to).is_some() => self.captures += 1,
            Move::Move { .. } => self.moves_to_empty += 1,
        }
    }
}

/// Find distinct immediate winning actions using line geometry directly.
pub fn direct_immediate_wins(position: &Position, attacker: Color, rules: Rules) -> ImmediateWins {
    if position.is_terminal() {
        return ImmediateWins::default();
    }

    let mut result = ImmediateWins::default();

    for line in WIN_LINES {
        let mut occupied_kinds = 0_u8;
        let mut attacker_count = 0_u8;
        let mut gap = None;

        for index in line {
            match position.board[index] {
                Some(piece) if piece.color == attacker => {
                    attacker_count += 1;
                    occupied_kinds |= 1 << piece.kind as u8;
                }
                _ => gap = Some(Square::from_index(index as u8).expect("winning-line square")),
            }
        }
        if attacker_count != 3 {
            continue;
        }

        let missing = PieceKind::ALL
            .into_iter()
            .find(|kind| occupied_kinds & (1 << *kind as u8) == 0)
            .expect("three unique pieces leave one missing kind");
        let target = gap.expect("three-piece line has one gap");
        let wanted = Piece {
            color: attacker,
            kind: missing,
        };

        let action = match position.piece_square(wanted) {
            None if position.at(target).is_none() => Some(Move::Place {
                piece: missing,
                to: target,
            }),
            None => None,
            Some(from)
                if position.opening_complete
                    && piece_can_move_to(position, from, target, wanted, rules) =>
            {
                Some(Move::Move { from, to: target })
            }
            Some(_) => None,
        };

        if let Some(action) = action {
            result.add(position, action);
        }
    }

    result
}

/// Find distinct immediate winning actions through the production move generator.
pub fn reference_immediate_wins(
    position: &Position,
    attacker: Color,
    rules: Rules,
) -> ImmediateWins {
    if position.is_terminal() {
        return ImmediateWins::default();
    }

    let mut counterfactual = position.clone();
    counterfactual.side_to_move = attacker;
    let mut result = ImmediateWins::default();
    for_each_legal_move(&counterfactual, rules, |action| {
        let child = counterfactual.play_unchecked(action);
        if child.winner() == Some(attacker) {
            result.add(position, action);
        }
    });
    result
}

fn piece_can_move_to(
    position: &Position,
    from: Square,
    to: Square,
    piece: Piece,
    rules: Rules,
) -> bool {
    if from == to
        || position
            .at(to)
            .is_some_and(|target| target.color == piece.color)
    {
        return false;
    }
    let file_delta = to.file() as i8 - from.file() as i8;
    let rank_delta = to.rank() as i8 - from.rank() as i8;

    match piece.kind {
        PieceKind::Pawn => {
            let travel = position.pawn_direction(piece.color).rank_delta();
            match position.at(to) {
                None => file_delta == 0 && rank_delta == travel,
                Some(_) => {
                    let initial = initial_pawn_direction(piece.color).rank_delta();
                    let capture_delta = match rules.returning_pawn_capture {
                        ReturningPawnCapture::OutboundOnly if travel != initial => return false,
                        ReturningPawnCapture::OutboundOnly
                        | ReturningPawnCapture::TowardOpponent => initial,
                        ReturningPawnCapture::TravelDirection => travel,
                    };
                    file_delta.abs() == 1 && rank_delta == capture_delta
                }
            }
        }
        PieceKind::Knight => {
            matches!((file_delta.abs(), rank_delta.abs()), (1, 2) | (2, 1))
        }
        PieceKind::Bishop => {
            file_delta.abs() == rank_delta.abs()
                && path_is_clear(position, from, to, file_delta.signum(), rank_delta.signum())
        }
        PieceKind::Rook => {
            (file_delta == 0 || rank_delta == 0)
                && path_is_clear(position, from, to, file_delta.signum(), rank_delta.signum())
        }
    }
}

fn path_is_clear(
    position: &Position,
    from: Square,
    to: Square,
    file_step: i8,
    rank_step: i8,
) -> bool {
    let mut cursor = from;
    loop {
        let Some(next) = cursor.offset(file_step, rank_step) else {
            return false;
        };
        if next == to {
            return true;
        }
        if position.at(next).is_some() {
            return false;
        }
        cursor = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PawnDirection;

    fn sq(file: u8, rank: u8) -> Square {
        Square::new(file, rank).unwrap()
    }

    fn position(
        cells: &[(Color, PieceKind, u8, u8)],
        side_to_move: Color,
        white_pawn: PawnDirection,
        black_pawn: PawnDirection,
        opening_complete: bool,
    ) -> Position {
        let mut board = [None; BOARD_CELLS];
        for &(color, kind, file, rank) in cells {
            board[sq(file, rank).index()] = Some(Piece { color, kind });
        }
        Position {
            board,
            side_to_move,
            pawn_directions: [white_pawn, black_pawn],
            opening_complete,
        }
    }

    fn assert_detectors_agree(position: &Position, attacker: Color, expected: ImmediateWins) {
        let direct = direct_immediate_wins(position, attacker, Rules::default());
        let reference = reference_immediate_wins(position, attacker, Rules::default());
        assert_eq!(direct, reference);
        assert_eq!(direct.actions, expected.actions);
        assert_eq!(direct.drops, expected.drops);
        assert_eq!(direct.moves_to_empty, expected.moves_to_empty);
        assert_eq!(direct.captures, expected.captures);
    }

    #[test]
    fn detects_drop_completion() {
        let p = position(
            &[
                (Color::Black, PieceKind::Pawn, 0, 0),
                (Color::Black, PieceKind::Knight, 1, 0),
                (Color::Black, PieceKind::Bishop, 2, 0),
            ],
            Color::White,
            PawnDirection::TowardBlack,
            PawnDirection::TowardWhite,
            true,
        );
        assert_detectors_agree(
            &p,
            Color::Black,
            ImmediateWins {
                actions: 1,
                drops: 1,
                ..ImmediateWins::default()
            },
        );
    }

    #[test]
    fn detects_move_to_empty_completion() {
        let p = position(
            &[
                (Color::Black, PieceKind::Pawn, 0, 0),
                (Color::Black, PieceKind::Knight, 1, 0),
                (Color::Black, PieceKind::Bishop, 2, 0),
                (Color::Black, PieceKind::Rook, 3, 3),
            ],
            Color::White,
            PawnDirection::TowardBlack,
            PawnDirection::TowardWhite,
            true,
        );
        assert_detectors_agree(
            &p,
            Color::Black,
            ImmediateWins {
                actions: 1,
                moves_to_empty: 1,
                ..ImmediateWins::default()
            },
        );
    }

    #[test]
    fn detects_capture_completion() {
        let p = position(
            &[
                (Color::Black, PieceKind::Pawn, 0, 0),
                (Color::Black, PieceKind::Knight, 1, 0),
                (Color::Black, PieceKind::Bishop, 2, 0),
                (Color::Black, PieceKind::Rook, 3, 3),
                (Color::White, PieceKind::Knight, 3, 0),
            ],
            Color::White,
            PawnDirection::TowardBlack,
            PawnDirection::TowardWhite,
            true,
        );
        assert_detectors_agree(
            &p,
            Color::Black,
            ImmediateWins {
                actions: 1,
                captures: 1,
                ..ImmediateWins::default()
            },
        );
    }

    #[test]
    fn blocked_slider_is_not_a_live_threat() {
        let p = position(
            &[
                (Color::Black, PieceKind::Pawn, 0, 0),
                (Color::Black, PieceKind::Knight, 1, 0),
                (Color::Black, PieceKind::Bishop, 2, 0),
                (Color::Black, PieceKind::Rook, 3, 3),
                (Color::White, PieceKind::Knight, 3, 2),
            ],
            Color::White,
            PawnDirection::TowardBlack,
            PawnDirection::TowardWhite,
            true,
        );
        assert_detectors_agree(&p, Color::Black, ImmediateWins::default());
    }

    #[test]
    fn opening_lock_allows_drop_but_not_movement() {
        let moving_piece = position(
            &[
                (Color::Black, PieceKind::Pawn, 0, 0),
                (Color::Black, PieceKind::Knight, 1, 0),
                (Color::Black, PieceKind::Bishop, 2, 0),
                (Color::Black, PieceKind::Rook, 3, 3),
            ],
            Color::White,
            PawnDirection::TowardBlack,
            PawnDirection::TowardWhite,
            false,
        );
        assert_detectors_agree(&moving_piece, Color::Black, ImmediateWins::default());

        let dropping_piece = position(
            &[
                (Color::Black, PieceKind::Pawn, 0, 0),
                (Color::Black, PieceKind::Knight, 1, 0),
                (Color::Black, PieceKind::Bishop, 2, 0),
            ],
            Color::White,
            PawnDirection::TowardBlack,
            PawnDirection::TowardWhite,
            false,
        );
        assert_eq!(
            direct_immediate_wins(&dropping_piece, Color::Black, Rules::default()).drops,
            1
        );
    }

    #[test]
    fn returning_pawn_capture_uses_travel_direction() {
        let p = position(
            &[
                (Color::Black, PieceKind::Knight, 0, 1),
                (Color::Black, PieceKind::Bishop, 1, 1),
                (Color::Black, PieceKind::Rook, 2, 1),
                (Color::Black, PieceKind::Pawn, 2, 0),
                (Color::White, PieceKind::Knight, 3, 1),
            ],
            Color::White,
            PawnDirection::TowardBlack,
            PawnDirection::TowardBlack,
            true,
        );
        assert_detectors_agree(
            &p,
            Color::Black,
            ImmediateWins {
                actions: 1,
                captures: 1,
                ..ImmediateWins::default()
            },
        );
    }

    #[test]
    fn direct_detector_matches_reference_across_dense_domain_samples() {
        use crate::ranking::{
            unrank_opening, unrank_post_opening, OpeningId, PostOpeningId, LOCKED_OPENING_DOMAIN,
            POST_OPENING_DOMAIN,
        };

        let mut random = 0x7468_7265_6174_u64;
        for _ in 0..50_000 {
            random = random
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let raw = (random % POST_OPENING_DOMAIN as u64) as u32;
            let position = unrank_post_opening(PostOpeningId::new(raw).unwrap());
            let attacker = position.side_to_move().opponent();
            assert_eq!(
                direct_immediate_wins(&position, attacker, Rules::default()),
                reference_immediate_wins(&position, attacker, Rules::default()),
                "post:{raw}"
            );
        }

        for _ in 0..10_000 {
            random = random
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let raw = (random % LOCKED_OPENING_DOMAIN as u64) as u32;
            let position = unrank_opening(OpeningId::new(raw).unwrap());
            let attacker = position.side_to_move().opponent();
            assert_eq!(
                direct_immediate_wins(&position, attacker, Rules::default()),
                reference_immediate_wins(&position, attacker, Rules::default()),
                "opening:{raw}"
            );
        }
    }
}
