//! Allocation-free forward edges for the normalized post-opening graph.
//!
//! This move generator is intentionally separate from [`Position::legal_moves`].
//! The vector-based implementation remains a readable reference oracle; tests
//! compare the two implementations before this kernel is used by the solver.

use crate::ranking::{
    rank_post_opening, swap_sides_and_rotate, unrank_post_opening, PostOpeningId,
    POST_OPENING_DOMAIN,
};
use crate::retrograde::GameGraph;
use crate::{
    initial_pawn_direction, Color, Move, PawnDirection, Piece, PieceKind, Position,
    ReturningPawnCapture, Rules, Square, BOARD_CELLS, BOARD_SIDE,
};

#[derive(Clone, Copy, Debug)]
pub struct PostOpeningGraph {
    rules: Rules,
}

impl PostOpeningGraph {
    pub const fn new(rules: Rules) -> Self {
        Self { rules }
    }

    pub const fn rules(self) -> Rules {
        self.rules
    }
}

impl GameGraph for PostOpeningGraph {
    fn node_count(&self) -> u32 {
        POST_OPENING_DOMAIN
    }

    fn is_terminal_loss(&self, node: u32) -> bool {
        unrank_post_opening(PostOpeningId::new(node).expect("graph node is in range")).is_terminal()
    }

    fn for_each_successor(&self, node: u32, mut emit: impl FnMut(u32)) {
        for_each_successor(
            PostOpeningId::new(node).expect("graph node is in range"),
            self.rules,
            |child| emit(child.get()),
        );
    }

    fn for_each_predecessor(&self, node: u32, mut emit: impl FnMut(u32)) {
        for_each_predecessor(
            PostOpeningId::new(node).expect("graph node is in range"),
            self.rules,
            |parent| emit(parent.get()),
        );
    }
}

/// Emit every legal action without allocating and return the action count.
///
/// The callback is never invoked for a terminal position. Different actions
/// always produce different child positions, so no edge de-duplication is
/// needed by [`for_each_successor`].
pub fn for_each_legal_move(position: &Position, rules: Rules, mut emit: impl FnMut(Move)) -> usize {
    visit_legal_moves(position, rules, &mut |action| {
        emit(action);
        true
    })
}

fn contains_legal_move(position: &Position, rules: Rules, wanted: Move) -> bool {
    let mut found = false;
    visit_legal_moves(position, rules, &mut |action| {
        found = action == wanted;
        !found
    });
    found
}

/// Visit legal moves until the callback returns `false` and return the number
/// of moves visited, including the move that stopped iteration.
fn visit_legal_moves(
    position: &Position,
    rules: Rules,
    emit: &mut impl FnMut(Move) -> bool,
) -> usize {
    if position.is_terminal() {
        return 0;
    }

    let mut count = 0;
    if !emit_placements(position, emit, &mut count) {
        return count;
    }
    if !position.opening_complete() {
        return count;
    }

    for index in 0..BOARD_CELLS as u8 {
        let from = square(index);
        let Some(piece) = position.at(from) else {
            continue;
        };
        if piece.color != position.side_to_move() {
            continue;
        }
        match piece.kind {
            PieceKind::Pawn => {
                if !emit_pawn_moves(position, from, piece, rules, emit, &mut count) {
                    return count;
                }
            }
            PieceKind::Knight => {
                for (file_delta, rank_delta) in [
                    (1, 2),
                    (2, 1),
                    (2, -1),
                    (1, -2),
                    (-1, -2),
                    (-2, -1),
                    (-2, 1),
                    (-1, 2),
                ] {
                    if let Some(to) = offset(from, file_delta, rank_delta) {
                        if position
                            .at(to)
                            .is_none_or(|target| target.color != piece.color)
                            && !push(Move::Move { from, to }, emit, &mut count)
                        {
                            return count;
                        }
                    }
                }
            }
            PieceKind::Bishop => {
                for direction in [(1, 1), (1, -1), (-1, -1), (-1, 1)] {
                    if !emit_slide(position, from, piece.color, direction, emit, &mut count) {
                        return count;
                    }
                }
            }
            PieceKind::Rook => {
                for direction in [(1, 0), (0, -1), (-1, 0), (0, 1)] {
                    if !emit_slide(position, from, piece.color, direction, emit, &mut count) {
                        return count;
                    }
                }
            }
        }
    }
    count
}

/// Decode one normalized state and emit its normalized child IDs.
pub fn for_each_successor(
    id: PostOpeningId,
    rules: Rules,
    mut emit: impl FnMut(PostOpeningId),
) -> usize {
    let position = unrank_post_opening(id);
    for_each_legal_move(&position, rules, |action| {
        let child = position.play_unchecked(action);
        let child_id = rank_post_opening(&child).expect("post-opening moves remain rankable");
        emit(child_id);
    })
}

/// Emit every normalized parent ID after reconstructing and forward-validating
/// each possible one-move predecessor.
///
/// The child ID is decoded in next-player coordinates, then transformed back
/// to the absolute Black-to-move position produced by a normalized White
/// parent. Candidate reversal uses inverse piece geometry while deliberately
/// over-generating pawn directions and capture history. A candidate is emitted
/// only when the production forward generator accepts its action and applying
/// that action reconstructs the exact absolute child.
pub fn for_each_predecessor(
    id: PostOpeningId,
    rules: Rules,
    mut emit: impl FnMut(PostOpeningId),
) -> usize {
    let child = swap_sides_and_rotate(&unrank_post_opening(id));
    debug_assert_eq!(child.side_to_move(), Color::Black);
    let mut count = 0;

    for to_index in 0..BOARD_CELLS as u8 {
        let to = square(to_index);
        let Some(mover) = child.at(to) else {
            continue;
        };
        if mover.color != Color::White {
            continue;
        }

        let mut placement_parent = child.clone();
        placement_parent.board[to.index()] = None;
        placement_parent.side_to_move = Color::White;
        if mover.kind == PieceKind::Pawn {
            placement_parent.pawn_directions[Color::White.index()] =
                initial_pawn_direction(Color::White);
        }
        validate_predecessor(
            placement_parent,
            Move::Place {
                piece: mover.kind,
                to,
            },
            &child,
            rules,
            &mut emit,
            &mut count,
        );

        for mover_direction in candidate_mover_directions(&child, mover) {
            for_each_reverse_origin(&child, to, mover, mover_direction, rules, false, |from| {
                let parent = reverse_move_candidate(
                    &child,
                    mover,
                    from,
                    to,
                    None,
                    mover_direction,
                    child.pawn_direction(Color::Black),
                );
                validate_predecessor(
                    parent,
                    Move::Move { from, to },
                    &child,
                    rules,
                    &mut emit,
                    &mut count,
                );
            });

            for captured_kind in PieceKind::ALL {
                let captured = Piece {
                    color: Color::Black,
                    kind: captured_kind,
                };
                if child.piece_square(captured).is_some() {
                    continue;
                }
                for captured_direction in candidate_captured_directions(&child, captured_kind) {
                    for_each_reverse_origin(
                        &child,
                        to,
                        mover,
                        mover_direction,
                        rules,
                        true,
                        |from| {
                            let parent = reverse_move_candidate(
                                &child,
                                mover,
                                from,
                                to,
                                Some(captured),
                                mover_direction,
                                captured_direction,
                            );
                            validate_predecessor(
                                parent,
                                Move::Move { from, to },
                                &child,
                                rules,
                                &mut emit,
                                &mut count,
                            );
                        },
                    );
                }
            }
        }
    }
    count
}

fn for_each_reverse_origin(
    child: &Position,
    to: Square,
    mover: Piece,
    mover_direction: PawnDirection,
    rules: Rules,
    is_capture: bool,
    mut emit: impl FnMut(Square),
) {
    match mover.kind {
        PieceKind::Pawn => {
            let travel = direction_delta(mover_direction);
            let rank_delta = if is_capture {
                let initial = 1;
                match rules.returning_pawn_capture {
                    ReturningPawnCapture::OutboundOnly if travel != initial => return,
                    ReturningPawnCapture::OutboundOnly | ReturningPawnCapture::TowardOpponent => {
                        initial
                    }
                    ReturningPawnCapture::TravelDirection => travel,
                }
            } else {
                travel
            };
            if is_capture {
                for file_delta in [-1, 1] {
                    if let Some(from) = offset(to, file_delta, -rank_delta) {
                        if child.at(from).is_none() {
                            emit(from);
                        }
                    }
                }
            } else if let Some(from) = offset(to, 0, -rank_delta) {
                if child.at(from).is_none() {
                    emit(from);
                }
            }
        }
        PieceKind::Knight => {
            for (file_delta, rank_delta) in [
                (1, 2),
                (2, 1),
                (2, -1),
                (1, -2),
                (-1, -2),
                (-2, -1),
                (-2, 1),
                (-1, 2),
            ] {
                if let Some(from) = offset(to, file_delta, rank_delta) {
                    if child.at(from).is_none() {
                        emit(from);
                    }
                }
            }
        }
        PieceKind::Bishop => {
            for direction in [(1, 1), (1, -1), (-1, -1), (-1, 1)] {
                emit_reverse_slide(child, to, direction, &mut emit);
            }
        }
        PieceKind::Rook => {
            for direction in [(1, 0), (0, -1), (-1, 0), (0, 1)] {
                emit_reverse_slide(child, to, direction, &mut emit);
            }
        }
    }
}

fn emit_reverse_slide(
    child: &Position,
    to: Square,
    (file_delta, rank_delta): (i8, i8),
    emit: &mut impl FnMut(Square),
) {
    let mut cursor = to;
    while let Some(from) = offset(cursor, file_delta, rank_delta) {
        if child.at(from).is_some() {
            break;
        }
        emit(from);
        cursor = from;
    }
}

fn reverse_move_candidate(
    child: &Position,
    mover: Piece,
    from: Square,
    to: Square,
    captured: Option<Piece>,
    mover_direction: PawnDirection,
    captured_direction: PawnDirection,
) -> Position {
    let mut parent = child.clone();
    parent.board[from.index()] = Some(mover);
    parent.board[to.index()] = captured;
    parent.side_to_move = Color::White;
    if mover.kind == PieceKind::Pawn {
        parent.pawn_directions[Color::White.index()] = mover_direction;
    }
    if captured.is_some_and(|piece| piece.kind == PieceKind::Pawn) {
        parent.pawn_directions[Color::Black.index()] = captured_direction;
    }
    parent
}

fn candidate_mover_directions(position: &Position, mover: Piece) -> CandidateDirections {
    if mover.kind == PieceKind::Pawn {
        CandidateDirections::both()
    } else {
        CandidateDirections::one(position.pawn_direction(Color::White))
    }
}

fn candidate_captured_directions(
    position: &Position,
    captured_kind: PieceKind,
) -> CandidateDirections {
    if captured_kind == PieceKind::Pawn {
        CandidateDirections::both()
    } else {
        CandidateDirections::one(position.pawn_direction(Color::Black))
    }
}

struct CandidateDirections {
    values: [PawnDirection; 2],
    length: usize,
}

impl CandidateDirections {
    fn one(direction: PawnDirection) -> Self {
        Self {
            values: [direction; 2],
            length: 1,
        }
    }

    fn both() -> Self {
        Self {
            values: [PawnDirection::TowardWhite, PawnDirection::TowardBlack],
            length: 2,
        }
    }
}

impl IntoIterator for CandidateDirections {
    type Item = PawnDirection;
    type IntoIter = std::iter::Take<std::array::IntoIter<PawnDirection, 2>>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter().take(self.length)
    }
}

fn validate_predecessor(
    parent: Position,
    action: Move,
    child: &Position,
    rules: Rules,
    emit: &mut impl FnMut(PostOpeningId),
    count: &mut usize,
) {
    let Some(parent_id) = rank_post_opening(&parent) else {
        return;
    };
    if !contains_legal_move(&parent, rules, action) || parent.play_unchecked(action) != *child {
        return;
    }

    *count += 1;
    emit(parent_id);
}

fn emit_placements(
    position: &Position,
    emit: &mut impl FnMut(Move) -> bool,
    count: &mut usize,
) -> bool {
    for kind in PieceKind::ALL {
        let piece = Piece {
            color: position.side_to_move(),
            kind,
        };
        if position.piece_square(piece).is_some() {
            continue;
        }
        for index in 0..BOARD_CELLS as u8 {
            let to = square(index);
            if position.at(to).is_none() && !push(Move::Place { piece: kind, to }, emit, count) {
                return false;
            }
        }
    }
    true
}

fn emit_pawn_moves(
    position: &Position,
    from: Square,
    piece: Piece,
    rules: Rules,
    emit: &mut impl FnMut(Move) -> bool,
    count: &mut usize,
) -> bool {
    let travel = direction_delta(position.pawn_direction(piece.color));
    if let Some(to) = offset(from, 0, travel) {
        if position.at(to).is_none() && !push(Move::Move { from, to }, emit, count) {
            return false;
        }
    }

    let initial = match piece.color {
        Color::White => 1,
        Color::Black => -1,
    };
    let capture_delta = match rules.returning_pawn_capture {
        ReturningPawnCapture::OutboundOnly if travel != initial => return true,
        ReturningPawnCapture::OutboundOnly | ReturningPawnCapture::TowardOpponent => initial,
        ReturningPawnCapture::TravelDirection => travel,
    };
    for file_delta in [-1, 1] {
        if let Some(to) = offset(from, file_delta, capture_delta) {
            if position
                .at(to)
                .is_some_and(|target| target.color != piece.color)
                && !push(Move::Move { from, to }, emit, count)
            {
                return false;
            }
        }
    }
    true
}

fn emit_slide(
    position: &Position,
    from: Square,
    color: Color,
    (file_delta, rank_delta): (i8, i8),
    emit: &mut impl FnMut(Move) -> bool,
    count: &mut usize,
) -> bool {
    let mut cursor = from;
    while let Some(to) = offset(cursor, file_delta, rank_delta) {
        match position.at(to) {
            None => {
                if !push(Move::Move { from, to }, emit, count) {
                    return false;
                }
            }
            Some(piece) if piece.color != color => {
                if !push(Move::Move { from, to }, emit, count) {
                    return false;
                }
                break;
            }
            Some(_) => break,
        }
        cursor = to;
    }
    true
}

fn push(action: Move, emit: &mut impl FnMut(Move) -> bool, count: &mut usize) -> bool {
    *count += 1;
    emit(action)
}

fn direction_delta(direction: PawnDirection) -> i8 {
    match direction {
        PawnDirection::TowardWhite => -1,
        PawnDirection::TowardBlack => 1,
    }
}

fn offset(square: Square, file_delta: i8, rank_delta: i8) -> Option<Square> {
    let file = square.file() as i8 + file_delta;
    let rank = square.rank() as i8 + rank_delta;
    if (0..BOARD_SIDE as i8).contains(&file) && (0..BOARD_SIDE as i8).contains(&rank) {
        Square::new(file as u8, rank as u8)
    } else {
        None
    }
}

fn square(index: u8) -> Square {
    Square::from_index(index).expect("board index is in range")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ranking::{normalize_player_to_move, POST_OPENING_DOMAIN};
    use std::collections::HashSet;

    const RULES: [Rules; 3] = [
        Rules::ORIGINAL_OUTBOUND_ONLY,
        Rules::ORIGINAL_TOWARD_OPPONENT,
        Rules::ORIGINAL_TRAVEL_DIRECTION,
    ];

    #[test]
    fn production_movegen_matches_reference_on_random_domain_positions() {
        let mut random = 0x1319_8a2e_0370_7344_u64;
        for sample in 0..20_000 {
            random = next_random(random);
            let id = PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap();
            let position = unrank_post_opening(id);
            for rules in RULES {
                assert_same_moves(&position, rules, sample);
            }
        }
    }

    #[test]
    fn production_movegen_matches_reference_in_locked_opening() {
        let mut layer = vec![Position::initial()];
        for ply in 0..=2 {
            for (sample, position) in layer.iter().enumerate() {
                assert_same_moves(position, Rules::default(), sample);
            }
            if ply < 2 {
                let mut children = Vec::new();
                for position in &layer {
                    for action in position.legal_moves(Rules::default()) {
                        children.push(position.play(action, Rules::default()).unwrap());
                    }
                }
                layer = children;
            }
        }
    }

    #[test]
    fn successor_ids_match_checked_reference_edges() {
        let mut random = 0xa409_3822_299f_31d0_u64;
        for sample in 0..20_000 {
            random = next_random(random);
            let id = PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap();
            let position = unrank_post_opening(id);

            let expected: HashSet<_> = position
                .legal_moves(Rules::default())
                .into_iter()
                .map(|action| {
                    let child = position.play(action, Rules::default()).unwrap();
                    rank_post_opening(&child).unwrap()
                })
                .collect();
            let mut actual = HashSet::new();
            let emitted = for_each_successor(id, Rules::default(), |child| {
                actual.insert(child);
            });
            assert_eq!(emitted, actual.len(), "duplicate edge at sample {sample}");
            assert_eq!(actual, expected, "successor mismatch at sample {sample}");
        }
    }

    #[test]
    fn successor_ids_are_in_next_player_coordinates() {
        let mut position = Position::initial();
        for (kind, index) in [
            (PieceKind::Rook, 0),
            (PieceKind::Rook, 15),
            (PieceKind::Bishop, 1),
            (PieceKind::Bishop, 14),
            (PieceKind::Knight, 2),
            (PieceKind::Knight, 13),
        ] {
            position = position
                .play(
                    Move::Place {
                        piece: kind,
                        to: square(index),
                    },
                    Rules::default(),
                )
                .unwrap();
        }
        let id = rank_post_opening(&position).unwrap();
        for_each_successor(id, Rules::default(), |child_id| {
            let child = unrank_post_opening(child_id);
            assert_eq!(child.side_to_move(), Color::White);
            assert_eq!(child, normalize_player_to_move(&child));
        });
    }

    #[test]
    fn sampled_forward_edges_are_recovered_as_predecessors() {
        let mut random = 0x4528_21e6_38d0_1377_u64;
        for sample in 0..5_000 {
            random = next_random(random);
            let parent_id =
                PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap();
            let parent = unrank_post_opening(parent_id);
            for rules in RULES {
                let actions = parent.legal_moves(rules);
                if actions.is_empty() {
                    continue;
                }
                random = next_random(random);
                let action = actions[random as usize % actions.len()];
                let child = parent.play(action, rules).unwrap();
                let child_id = rank_post_opening(&child).unwrap();
                let mut recovered = false;
                for_each_predecessor(child_id, rules, |candidate| {
                    recovered |= candidate == parent_id;
                });
                assert!(recovered, "missed forward edge at sample {sample}");
            }
        }
    }

    #[test]
    fn emitted_predecessors_replay_through_reference_engine() {
        let mut random = 0xbe54_66cf_34e9_0c6c_u64;
        for sample in 0..2_000 {
            random = next_random(random);
            let child_id =
                PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap();
            for rules in RULES {
                let mut predecessors = HashSet::new();
                let emitted = for_each_predecessor(child_id, rules, |parent| {
                    predecessors.insert(parent);
                });
                assert_eq!(
                    emitted,
                    predecessors.len(),
                    "duplicate predecessor at sample {sample}"
                );

                for parent_id in predecessors {
                    let parent = unrank_post_opening(parent_id);
                    let reference_children: HashSet<_> = parent
                        .legal_moves(rules)
                        .into_iter()
                        .map(|action| {
                            rank_post_opening(&parent.play(action, rules).unwrap()).unwrap()
                        })
                        .collect();
                    assert!(
                        reference_children.contains(&child_id),
                        "predecessor does not replay at sample {sample}"
                    );
                }
            }
        }
    }

    #[test]
    fn post_opening_graph_adapter_preserves_dense_edges() {
        let graph = PostOpeningGraph::new(Rules::default());
        assert_eq!(graph.node_count(), POST_OPENING_DOMAIN);
        assert_eq!(graph.rules(), Rules::default());

        let mut random = 0xc0ac_29b7_c97c_50dd_u64;
        for _ in 0..1_000 {
            random = next_random(random);
            let id = PostOpeningId::new((random % POST_OPENING_DOMAIN as u64) as u32).unwrap();
            let mut expected = Vec::new();
            for_each_successor(id, Rules::default(), |child| expected.push(child.get()));
            let mut actual = Vec::new();
            graph.for_each_successor(id.get(), |child| actual.push(child));
            assert_eq!(actual, expected);
            assert_eq!(
                graph.is_terminal_loss(id.get()),
                unrank_post_opening(id).is_terminal()
            );
        }
    }

    fn assert_same_moves(position: &Position, rules: Rules, sample: usize) {
        let expected: HashSet<_> = position.legal_moves(rules).into_iter().collect();
        let mut actual = HashSet::new();
        let emitted = for_each_legal_move(position, rules, |action| {
            actual.insert(action);
        });
        assert_eq!(emitted, actual.len(), "duplicate move at sample {sample}");
        assert_eq!(actual, expected, "move mismatch at sample {sample}");
    }

    fn next_random(state: u64) -> u64 {
        state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407)
    }
}
