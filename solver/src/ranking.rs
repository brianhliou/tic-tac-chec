//! Collision-free ranks for the locked opening and normalized post-opening game.
//!
//! The post-opening rank removes absolute side to move: Black-to-move positions
//! are color-swapped and rotated 180 degrees, so the player to move is always
//! represented as White. This makes the exact domain fit in `u32`.

use super::{
    initial_pawn_direction, Color, PawnDirection, Piece, PieceKind, Position, BOARD_CELLS,
    BOARD_SIDE,
};

pub const POST_OPENING_DOMAIN: u32 = 2_462_360_745;
pub const LOCKED_OPENING_DOMAIN: u32 = 14_236_865;

pub fn opening_ply_range(ply: u8) -> Option<std::ops::Range<u32>> {
    (ply <= 5).then(|| OPENING_OFFSETS[ply as usize]..OPENING_OFFSETS[ply as usize + 1])
}

const PIECES: [Piece; 8] = [
    Piece {
        color: Color::White,
        kind: PieceKind::Pawn,
    },
    Piece {
        color: Color::White,
        kind: PieceKind::Knight,
    },
    Piece {
        color: Color::White,
        kind: PieceKind::Bishop,
    },
    Piece {
        color: Color::White,
        kind: PieceKind::Rook,
    },
    Piece {
        color: Color::Black,
        kind: PieceKind::Pawn,
    },
    Piece {
        color: Color::Black,
        kind: PieceKind::Knight,
    },
    Piece {
        color: Color::Black,
        kind: PieceKind::Bishop,
    },
    Piece {
        color: Color::Black,
        kind: PieceKind::Rook,
    },
];
const NON_PAWN_INDICES: [usize; 6] = [1, 2, 3, 5, 6, 7];

const SUBSET_OFFSETS: [u32; 257] = subset_offsets();
const OPENING_OFFSETS: [u32; 7] = opening_offsets();
const PAIR_BLOCK_OFFSETS: [u16; 25] = pair_block_offsets();

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PostOpeningId(u32);

impl PostOpeningId {
    pub const fn new(raw: u32) -> Option<Self> {
        if raw < POST_OPENING_DOMAIN {
            Some(Self(raw))
        } else {
            None
        }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpeningId(u32);

impl OpeningId {
    pub const fn new(raw: u32) -> Option<Self> {
        if raw < LOCKED_OPENING_DOMAIN {
            Some(Self(raw))
        } else {
            None
        }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Return the equivalent position with the player to move represented as White.
pub fn normalize_player_to_move(position: &Position) -> Position {
    if position.side_to_move == Color::White {
        return position.clone();
    }

    swap_sides_and_rotate(position)
}

/// Apply the exact color-swap plus 180-degree rotation symmetry.
///
/// This toggles the absolute side to move and is its own inverse. The solver
/// uses it after each normalized move to restore player-to-move coordinates.
pub fn swap_sides_and_rotate(position: &Position) -> Position {
    let mut board = [None; BOARD_CELLS];
    for (index, cell) in position.board.iter().enumerate() {
        if let Some(piece) = cell {
            board[BOARD_CELLS - 1 - index] = Some(Piece {
                color: piece.color.opponent(),
                kind: piece.kind,
            });
        }
    }
    Position {
        board,
        side_to_move: position.side_to_move.opponent(),
        pawn_directions: [
            position.pawn_directions[Color::Black.index()].reversed(),
            position.pawn_directions[Color::White.index()].reversed(),
        ],
        opening_complete: position.opening_complete,
    }
}

/// Rank a post-opening position after exact player-to-move normalization.
///
/// Returns `None` for locked-opening positions or structurally invalid pawn
/// directions. Every position produced by the rules engine after the opening is
/// rankable.
pub fn rank_post_opening(position: &Position) -> Option<PostOpeningId> {
    if !position.opening_complete {
        return None;
    }
    let position = normalize_player_to_move(position);
    let locations = piece_locations(&position)?;
    validate_hand_pawns(&position, &locations)?;

    let mut subset = 0_u8;
    for (index, location) in locations.iter().enumerate() {
        if location.is_some() {
            subset |= 1 << index;
        }
    }

    let (pawn_rank, mut available) = rank_pawns(&position, &locations, subset)?;
    let selected_non_pawns = selected_indices(subset, &NON_PAWN_INDICES);
    let injection_rank = rank_injection(
        &locations,
        &selected_non_pawns.0,
        selected_non_pawns.1,
        &mut available,
    )?;
    let injection_count = permutations(16 - subset_pawn_count(subset), selected_non_pawns.1);
    let within = pawn_rank as u32 * injection_count + injection_rank;
    let raw = SUBSET_OFFSETS[subset as usize] + within;
    debug_assert!(raw < SUBSET_OFFSETS[subset as usize + 1]);
    Some(PostOpeningId(raw))
}

/// Decode a normalized post-opening position. The returned side to move is White.
pub fn unrank_post_opening(id: PostOpeningId) -> Position {
    let raw = id.get();
    let subset = subset_for_rank(raw);
    let mut within = raw - SUBSET_OFFSETS[subset as usize];
    let selected_non_pawns = selected_indices(subset, &NON_PAWN_INDICES);
    let injection_count = permutations(16 - subset_pawn_count(subset), selected_non_pawns.1);
    let pawn_rank = within / injection_count;
    within %= injection_count;

    let mut board = [None; BOARD_CELLS];
    let mut pawn_directions = [
        initial_pawn_direction(Color::White),
        initial_pawn_direction(Color::Black),
    ];
    let mut available = unrank_pawns(subset, pawn_rank as u16, &mut board, &mut pawn_directions);
    unrank_injection(
        within,
        &selected_non_pawns.0,
        selected_non_pawns.1,
        &mut available,
        &mut board,
    );

    Position {
        board,
        side_to_move: Color::White,
        pawn_directions,
        opening_complete: true,
    }
}

/// Rank one of the six placement-only opening plies (0 through 5).
pub fn rank_opening(position: &Position) -> Option<OpeningId> {
    if position.opening_complete {
        return None;
    }
    let locations = piece_locations(position)?;
    validate_opening_pawns(position, &locations)?;

    let white_mask = color_piece_mask(&locations, Color::White);
    let black_mask = color_piece_mask(&locations, Color::Black);
    let white = white_mask.count_ones() as usize;
    let black = black_mask.count_ones() as usize;
    let ply = white + black;
    if ply > 5
        || white != ply.div_ceil(2)
        || black != ply / 2
        || position.side_to_move
            != if ply.is_multiple_of(2) {
                Color::White
            } else {
                Color::Black
            }
    {
        return None;
    }

    let white_subset_rank = combination_mask_rank(white_mask, white)?;
    let black_subset_rank = combination_mask_rank(black_mask, black)?;
    let mut selected = [0_usize; 8];
    let mut selected_len = 0;
    for (index, location) in locations.iter().enumerate() {
        if location.is_some() {
            selected[selected_len] = index;
            selected_len += 1;
        }
    }
    let mut available = full_available();
    let injection_rank = rank_injection(&locations, &selected, selected_len, &mut available)?;
    let black_subset_count = choose(4, black);
    let within = (white_subset_rank * black_subset_count + black_subset_rank)
        * permutations(16, ply)
        + injection_rank;
    Some(OpeningId(OPENING_OFFSETS[ply] + within))
}

/// Decode a locked-opening position with its absolute side to move.
pub fn unrank_opening(id: OpeningId) -> Position {
    let raw = id.get();
    let ply = (0..6)
        .find(|&candidate| raw < OPENING_OFFSETS[candidate + 1])
        .expect("opening id is in range");
    let white = ply.div_ceil(2);
    let black = ply / 2;
    let injection_count = permutations(16, ply);
    let mut within = raw - OPENING_OFFSETS[ply];
    let injection_rank = within % injection_count;
    within /= injection_count;
    let black_subset_count = choose(4, black);
    let black_subset_rank = within % black_subset_count;
    let white_subset_rank = within / black_subset_count;
    let white_mask = combination_mask_unrank(white, white_subset_rank);
    let black_mask = combination_mask_unrank(black, black_subset_rank);

    let mut selected = [0_usize; 8];
    let mut selected_len = 0;
    for kind in 0..4 {
        if white_mask & (1 << kind) != 0 {
            selected[selected_len] = kind;
            selected_len += 1;
        }
    }
    for kind in 0..4 {
        if black_mask & (1 << kind) != 0 {
            selected[selected_len] = 4 + kind;
            selected_len += 1;
        }
    }

    let mut board = [None; BOARD_CELLS];
    let mut available = full_available();
    unrank_injection(
        injection_rank,
        &selected,
        selected_len,
        &mut available,
        &mut board,
    );
    let mut position = Position {
        board,
        side_to_move: if ply.is_multiple_of(2) {
            Color::White
        } else {
            Color::Black
        },
        pawn_directions: [
            initial_pawn_direction(Color::White),
            initial_pawn_direction(Color::Black),
        ],
        opening_complete: false,
    };
    set_opening_pawn_directions(&mut position);
    position
}

fn rank_pawns(
    position: &Position,
    locations: &[Option<u8>; 8],
    subset: u8,
) -> Option<(u16, AvailableSquares)> {
    let white_on = subset & 1 != 0;
    let black_on = subset & (1 << 4) != 0;
    let mut available = full_available();
    match (white_on, black_on) {
        (false, false) => Some((0, available)),
        (true, false) => {
            let square = locations[0]?;
            let option = oriented_option(square, position.pawn_directions[0])?;
            available.remove_square(square)?;
            Some((option as u16, available))
        }
        (false, true) => {
            let square = locations[4]?;
            let option = oriented_option(square, position.pawn_directions[1])?;
            available.remove_square(square)?;
            Some((option as u16, available))
        }
        (true, true) => {
            let white_square = locations[0]?;
            let black_square = locations[4]?;
            if white_square == black_square {
                return None;
            }
            let white_option = oriented_option(white_square, position.pawn_directions[0])?;
            let black_option = oriented_option(black_square, position.pawn_directions[1])?;
            let rank = oriented_pair_rank(white_option, black_option)?;
            available.remove_square(white_square)?;
            available.remove_square(black_square)?;
            Some((rank, available))
        }
    }
}

fn unrank_pawns(
    subset: u8,
    pawn_rank: u16,
    board: &mut [Option<Piece>; BOARD_CELLS],
    directions: &mut [PawnDirection; 2],
) -> AvailableSquares {
    let white_on = subset & 1 != 0;
    let black_on = subset & (1 << 4) != 0;
    let mut available = full_available();
    match (white_on, black_on) {
        (false, false) => debug_assert_eq!(pawn_rank, 0),
        (true, false) => {
            let (square, direction) = oriented_option_decode(pawn_rank as u8);
            place_decoded_pawn(0, square, direction, board, directions, &mut available);
        }
        (false, true) => {
            let (square, direction) = oriented_option_decode(pawn_rank as u8);
            place_decoded_pawn(4, square, direction, board, directions, &mut available);
        }
        (true, true) => {
            let (white_option, black_option) = oriented_pair_unrank(pawn_rank);
            let (white_square, white_direction) = oriented_option_decode(white_option);
            let (black_square, black_direction) = oriented_option_decode(black_option);
            place_decoded_pawn(
                0,
                white_square,
                white_direction,
                board,
                directions,
                &mut available,
            );
            place_decoded_pawn(
                4,
                black_square,
                black_direction,
                board,
                directions,
                &mut available,
            );
        }
    }
    available
}

fn place_decoded_pawn(
    piece_index: usize,
    square: u8,
    direction: PawnDirection,
    board: &mut [Option<Piece>; BOARD_CELLS],
    directions: &mut [PawnDirection; 2],
    available: &mut AvailableSquares,
) {
    board[square as usize] = Some(PIECES[piece_index]);
    directions[PIECES[piece_index].color.index()] = direction;
    available
        .remove_square(square)
        .expect("decoded pawn squares are distinct");
}

fn piece_locations(position: &Position) -> Option<[Option<u8>; 8]> {
    let mut locations = [None; 8];
    for (square, cell) in position.board.iter().enumerate() {
        let Some(piece) = cell else {
            continue;
        };
        let index = piece_index(*piece);
        if locations[index].replace(square as u8).is_some() {
            return None;
        }
    }
    Some(locations)
}

fn piece_index(piece: Piece) -> usize {
    piece.color.index() * 4 + piece.kind as usize
}

fn validate_hand_pawns(position: &Position, locations: &[Option<u8>; 8]) -> Option<()> {
    for (color, index) in [(Color::White, 0), (Color::Black, 4)] {
        if locations[index].is_none()
            && position.pawn_directions[color.index()] != initial_pawn_direction(color)
        {
            return None;
        }
    }
    Some(())
}

fn validate_opening_pawns(position: &Position, locations: &[Option<u8>; 8]) -> Option<()> {
    for (color, index) in [(Color::White, 0), (Color::Black, 4)] {
        let expected = locations[index].map_or(initial_pawn_direction(color), |square| {
            opening_pawn_direction(color, square)
        });
        if position.pawn_directions[color.index()] != expected {
            return None;
        }
    }
    Some(())
}

fn opening_pawn_direction(color: Color, square: u8) -> PawnDirection {
    let initial = initial_pawn_direction(color);
    let rank = square / BOARD_SIDE;
    match (initial, rank) {
        (PawnDirection::TowardWhite, 0) | (PawnDirection::TowardBlack, 3) => initial.reversed(),
        _ => initial,
    }
}

fn set_opening_pawn_directions(position: &mut Position) {
    for (color, piece_index) in [(Color::White, 0), (Color::Black, 4)] {
        if let Some((square, _)) = position
            .board
            .iter()
            .enumerate()
            .find(|(_, cell)| **cell == Some(PIECES[piece_index]))
        {
            position.pawn_directions[color.index()] = opening_pawn_direction(color, square as u8);
        }
    }
}

fn color_piece_mask(locations: &[Option<u8>; 8], color: Color) -> u8 {
    let base = color.index() * 4;
    let mut mask = 0;
    for kind in 0..4 {
        if locations[base + kind].is_some() {
            mask |= 1 << kind;
        }
    }
    mask
}

fn selected_indices(subset: u8, candidates: &[usize]) -> ([usize; 8], usize) {
    let mut selected = [0; 8];
    let mut len = 0;
    for &index in candidates {
        if subset & (1 << index) != 0 {
            selected[len] = index;
            len += 1;
        }
    }
    (selected, len)
}

#[derive(Clone, Copy)]
struct AvailableSquares {
    squares: [u8; BOARD_CELLS],
    len: usize,
}

impl AvailableSquares {
    fn remove_at(&mut self, index: usize) -> u8 {
        let square = self.squares[index];
        self.squares.copy_within(index + 1..self.len, index);
        self.len -= 1;
        square
    }

    fn remove_square(&mut self, square: u8) -> Option<()> {
        let index = self.squares[..self.len]
            .iter()
            .position(|&candidate| candidate == square)?;
        self.remove_at(index);
        Some(())
    }
}

fn full_available() -> AvailableSquares {
    let mut squares = [0; BOARD_CELLS];
    for (index, square) in squares.iter_mut().enumerate() {
        *square = index as u8;
    }
    AvailableSquares {
        squares,
        len: BOARD_CELLS,
    }
}

fn rank_injection(
    locations: &[Option<u8>; 8],
    selected: &[usize; 8],
    selected_len: usize,
    available: &mut AvailableSquares,
) -> Option<u32> {
    let mut rank = 0;
    for (ordinal, &piece_index) in selected[..selected_len].iter().enumerate() {
        let square = locations[piece_index]?;
        let digit = available.squares[..available.len]
            .iter()
            .position(|&candidate| candidate == square)?;
        let pieces_after = selected_len - ordinal - 1;
        rank += digit as u32 * permutations(available.len - 1, pieces_after);
        available.remove_at(digit);
    }
    Some(rank)
}

fn unrank_injection(
    mut rank: u32,
    selected: &[usize; 8],
    selected_len: usize,
    available: &mut AvailableSquares,
    board: &mut [Option<Piece>; BOARD_CELLS],
) {
    for (ordinal, &piece_index) in selected[..selected_len].iter().enumerate() {
        let pieces_after = selected_len - ordinal - 1;
        let block = permutations(available.len - 1, pieces_after);
        let digit = (rank / block) as usize;
        rank %= block;
        let square = available.remove_at(digit);
        board[square as usize] = Some(PIECES[piece_index]);
    }
    debug_assert_eq!(rank, 0);
}

fn oriented_option(square: u8, direction: PawnDirection) -> Option<u8> {
    match square / BOARD_SIDE {
        0 if direction == PawnDirection::TowardBlack => Some(square),
        1 | 2 => Some(4 + (square - 4) * 2 + u8::from(direction == PawnDirection::TowardBlack)),
        3 if direction == PawnDirection::TowardWhite => Some(20 + square - 12),
        _ => None,
    }
}

fn oriented_option_decode(option: u8) -> (u8, PawnDirection) {
    match option {
        0..=3 => (option, PawnDirection::TowardBlack),
        4..=19 => (
            4 + (option - 4) / 2,
            if (option - 4).is_multiple_of(2) {
                PawnDirection::TowardWhite
            } else {
                PawnDirection::TowardBlack
            },
        ),
        20..=23 => (12 + option - 20, PawnDirection::TowardWhite),
        _ => unreachable!("oriented pawn option is in 0..24"),
    }
}

const fn oriented_option_start(square: u8) -> u8 {
    match square / BOARD_SIDE {
        0 => square,
        1 | 2 => 4 + (square - 4) * 2,
        3 => 20 + square - 12,
        _ => unreachable!(),
    }
}

const fn oriented_option_count(square: u8) -> u8 {
    if square / BOARD_SIDE == 1 || square / BOARD_SIDE == 2 {
        2
    } else {
        1
    }
}

fn oriented_pair_rank(white_option: u8, black_option: u8) -> Option<u16> {
    let (white_square, _) = oriented_option_decode(white_option);
    let (black_square, _) = oriented_option_decode(black_option);
    if white_square == black_square {
        return None;
    }
    let skipped_start = oriented_option_start(white_square);
    let skipped_count = oriented_option_count(white_square);
    let compressed_black = if black_option < skipped_start {
        black_option
    } else {
        black_option - skipped_count
    };
    Some(PAIR_BLOCK_OFFSETS[white_option as usize] + compressed_black as u16)
}

fn oriented_pair_unrank(rank: u16) -> (u8, u8) {
    let white_option = (0..24)
        .find(|&option| rank < PAIR_BLOCK_OFFSETS[option + 1])
        .expect("oriented pawn pair rank is in range") as u8;
    let local = (rank - PAIR_BLOCK_OFFSETS[white_option as usize]) as u8;
    let (white_square, _) = oriented_option_decode(white_option);
    let skipped_start = oriented_option_start(white_square);
    let skipped_count = oriented_option_count(white_square);
    let black_option = if local < skipped_start {
        local
    } else {
        local + skipped_count
    };
    (white_option, black_option)
}

fn combination_mask_rank(mask: u8, count: usize) -> Option<u32> {
    if mask >= 16 || mask.count_ones() as usize != count {
        return None;
    }
    let mut rank = 0;
    for candidate in 0..mask {
        if candidate.count_ones() as usize == count {
            rank += 1;
        }
    }
    Some(rank)
}

fn combination_mask_unrank(count: usize, rank: u32) -> u8 {
    let mut seen = 0;
    for mask in 0_u8..16 {
        if mask.count_ones() as usize == count {
            if seen == rank {
                return mask;
            }
            seen += 1;
        }
    }
    unreachable!("combination rank is in range")
}

fn subset_for_rank(rank: u32) -> u8 {
    let mut low = 0_usize;
    let mut high = 256_usize;
    while low + 1 < high {
        let middle = (low + high) / 2;
        if SUBSET_OFFSETS[middle] <= rank {
            low = middle;
        } else {
            high = middle;
        }
    }
    low as u8
}

const fn subset_pawn_count(subset: u8) -> usize {
    let white = if subset & 1 != 0 { 1 } else { 0 };
    let black = if subset & (1 << 4) != 0 { 1 } else { 0 };
    white + black
}

const fn subset_count(subset: u8) -> u32 {
    let pawns = subset_pawn_count(subset);
    let non_pawns = subset.count_ones() as usize - pawns;
    match pawns {
        0 => permutations_const(16, non_pawns),
        1 => 24 * permutations_const(15, non_pawns),
        2 => 536 * permutations_const(14, non_pawns),
        _ => unreachable!(),
    }
}

const fn subset_offsets() -> [u32; 257] {
    let mut offsets = [0; 257];
    let mut subset = 0;
    while subset < 256 {
        offsets[subset + 1] = offsets[subset] + subset_count(subset as u8);
        subset += 1;
    }
    assert!(offsets[256] == POST_OPENING_DOMAIN);
    offsets
}

const fn opening_offsets() -> [u32; 7] {
    let mut offsets = [0; 7];
    let mut ply: usize = 0;
    while ply <= 5 {
        let white = ply.div_ceil(2);
        let black = ply / 2;
        offsets[ply + 1] = offsets[ply]
            + choose_const(4, white) * choose_const(4, black) * permutations_const(16, ply);
        ply += 1;
    }
    assert!(offsets[6] == LOCKED_OPENING_DOMAIN);
    offsets
}

const fn pair_block_offsets() -> [u16; 25] {
    let mut offsets = [0; 25];
    let mut option = 0;
    while option < 24 {
        let square = if option < 4 {
            option
        } else if option < 20 {
            4 + (option - 4) / 2
        } else {
            12 + option - 20
        };
        offsets[option as usize + 1] =
            offsets[option as usize] + (24 - oriented_option_count(square)) as u16;
        option += 1;
    }
    assert!(offsets[24] == 536);
    offsets
}

fn choose(n: usize, k: usize) -> u32 {
    choose_const(n, k)
}

const fn choose_const(n: usize, k: usize) -> u32 {
    if k > n {
        return 0;
    }
    let k = if k < n - k { k } else { n - k };
    let mut result = 1_u32;
    let mut i = 0;
    while i < k {
        result = result * (n - i) as u32 / (i + 1) as u32;
        i += 1;
    }
    result
}

fn permutations(n: usize, k: usize) -> u32 {
    permutations_const(n, k)
}

const fn permutations_const(n: usize, k: usize) -> u32 {
    let mut result = 1_u32;
    let mut i = 0;
    while i < k {
        result *= (n - i) as u32;
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Move, Rules, Square};
    use std::collections::HashSet;

    fn play(position: &Position, action: Move) -> Position {
        position.play(action, Rules::default()).unwrap()
    }

    fn square(index: u8) -> Square {
        Square::from_index(index).unwrap()
    }

    fn opened_position() -> Position {
        let mut position = Position::initial();
        for (kind, to) in [
            (PieceKind::Rook, 0),
            (PieceKind::Rook, 15),
            (PieceKind::Bishop, 1),
            (PieceKind::Bishop, 14),
            (PieceKind::Knight, 2),
            (PieceKind::Knight, 13),
        ] {
            position = play(
                &position,
                Move::Place {
                    piece: kind,
                    to: square(to),
                },
            );
        }
        position
    }

    #[test]
    fn domain_constants_match_bucket_offsets() {
        assert_eq!(SUBSET_OFFSETS[256], POST_OPENING_DOMAIN);
        assert_eq!(OPENING_OFFSETS[6], LOCKED_OPENING_DOMAIN);
        assert_eq!(PAIR_BLOCK_OFFSETS[24], 536);
    }

    #[test]
    fn opening_ply_ranges_partition_the_domain() {
        let expected = [1, 64, 3_840, 80_640, 1_572_480, 12_579_840];
        let mut end = 0;
        for (ply, expected_length) in expected.into_iter().enumerate() {
            let range = opening_ply_range(ply as u8).unwrap();
            assert_eq!(range.start, end);
            assert_eq!(range.len(), expected_length);
            end = range.end;
        }
        assert_eq!(end, LOCKED_OPENING_DOMAIN);
        assert!(opening_ply_range(6).is_none());
    }

    #[test]
    fn every_oriented_pawn_option_round_trips() {
        for option in 0..24 {
            let (square, direction) = oriented_option_decode(option);
            assert_eq!(oriented_option(square, direction), Some(option));
        }
        for rank in 0..536 {
            let (white, black) = oriented_pair_unrank(rank);
            assert_eq!(oriented_pair_rank(white, black), Some(rank));
        }
    }

    #[test]
    fn post_opening_subset_boundaries_round_trip() {
        for subset in 0..256 {
            let start = SUBSET_OFFSETS[subset];
            let end = SUBSET_OFFSETS[subset + 1] - 1;
            for raw in [start, end] {
                let id = PostOpeningId::new(raw).unwrap();
                let position = unrank_post_opening(id);
                assert_eq!(rank_post_opening(&position), Some(id));
            }
        }
    }

    #[test]
    fn post_opening_random_ids_round_trip() {
        let mut state = 0x4d59_5df4_d0f3_3173_u64;
        for _ in 0..100_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let raw = (state % POST_OPENING_DOMAIN as u64) as u32;
            let id = PostOpeningId::new(raw).unwrap();
            assert_eq!(rank_post_opening(&unrank_post_opening(id)), Some(id));
        }
    }

    #[test]
    fn black_to_move_normalizes_without_changing_rank() {
        let position = play(
            &opened_position(),
            Move::Move {
                from: square(0),
                to: square(4),
            },
        );
        assert_eq!(position.side_to_move(), Color::Black);
        let normalized = normalize_player_to_move(&position);
        assert_eq!(normalized.side_to_move(), Color::White);
        assert_eq!(rank_post_opening(&position), rank_post_opening(&normalized));
        assert_eq!(
            unrank_post_opening(rank_post_opening(&position).unwrap()),
            normalized
        );
        assert_eq!(
            swap_sides_and_rotate(&swap_sides_and_rotate(&position)),
            position
        );
    }

    #[test]
    fn opening_prefix_is_exhaustively_bijective() {
        for raw in 0..OPENING_OFFSETS[4] {
            let id = OpeningId::new(raw).unwrap();
            assert_eq!(rank_opening(&unrank_opening(id)), Some(id));
        }
    }

    #[test]
    fn engine_generated_opening_layers_have_exact_unique_ranks() {
        let expected = [1, 64, 3_840, 80_640];
        let mut layer = vec![Position::initial()];
        for (ply, &expected_count) in expected.iter().enumerate() {
            let ids: HashSet<_> = layer
                .iter()
                .map(|position| rank_opening(position).unwrap())
                .collect();
            assert_eq!(ids.len(), expected_count, "opening ply {ply}");
            for &id in &ids {
                let position = unrank_opening(id);
                assert_eq!(rank_opening(&position), Some(id));
            }

            if ply + 1 < expected.len() {
                let mut next_ids = HashSet::new();
                for position in &layer {
                    for action in position.legal_moves(Rules::default()) {
                        let child = play(position, action);
                        next_ids.insert(rank_opening(&child).unwrap());
                    }
                }
                layer = next_ids.into_iter().map(unrank_opening).collect();
            }
        }
    }

    #[test]
    fn engine_generated_post_opening_walks_round_trip() {
        let mut position = opened_position();
        let mut random = 0xe703_7ed1_a0b4_28db_u64;
        for _ in 0..20_000 {
            let id = rank_post_opening(&position).unwrap();
            assert_eq!(unrank_post_opening(id), normalize_player_to_move(&position));

            let moves = position.legal_moves(Rules::default());
            if moves.is_empty() {
                position = opened_position();
                continue;
            }
            random = random
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            position = play(&position, moves[(random as usize) % moves.len()]);
        }
    }

    #[test]
    fn opening_boundaries_and_random_ids_round_trip() {
        for ply in 0..6 {
            for raw in [OPENING_OFFSETS[ply], OPENING_OFFSETS[ply + 1] - 1] {
                let id = OpeningId::new(raw).unwrap();
                assert_eq!(rank_opening(&unrank_opening(id)), Some(id));
            }
        }

        let mut state = 0xa076_1d64_78bd_642f_u64;
        for _ in 0..100_000 {
            state ^= state >> 7;
            state ^= state << 9;
            let raw = (state % LOCKED_OPENING_DOMAIN as u64) as u32;
            let id = OpeningId::new(raw).unwrap();
            assert_eq!(rank_opening(&unrank_opening(id)), Some(id));
        }
    }

    #[test]
    fn rank_spaces_reject_wrong_phase_and_out_of_range_ids() {
        assert_eq!(rank_post_opening(&Position::initial()), None);
        assert_eq!(rank_opening(&opened_position()), None);
        assert_eq!(PostOpeningId::new(POST_OPENING_DOMAIN), None);
        assert_eq!(OpeningId::new(LOCKED_OPENING_DOMAIN), None);
    }
}
