use std::fmt;

pub mod checkpoint;
pub mod graph;
pub mod ranking;
pub mod retrograde;

pub const BOARD_SIDE: u8 = 4;
pub const BOARD_CELLS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    pub const ALL: [Self; 2] = [Self::White, Self::Black];

    pub const fn opponent(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PieceKind {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
}

impl PieceKind {
    pub const ALL: [Self; 4] = [Self::Pawn, Self::Knight, Self::Bishop, Self::Rook];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Square(u8);

impl Square {
    pub const fn new(file: u8, rank: u8) -> Option<Self> {
        if file < BOARD_SIDE && rank < BOARD_SIDE {
            Some(Self(rank * BOARD_SIDE + file))
        } else {
            None
        }
    }

    pub const fn from_index(index: u8) -> Option<Self> {
        if index < BOARD_CELLS as u8 {
            Some(Self(index))
        } else {
            None
        }
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn file(self) -> u8 {
        self.0 % BOARD_SIDE
    }

    pub const fn rank(self) -> u8 {
        self.0 / BOARD_SIDE
    }

    fn offset(self, file_delta: i8, rank_delta: i8) -> Option<Self> {
        let file = self.file() as i8 + file_delta;
        let rank = self.rank() as i8 + rank_delta;
        if (0..BOARD_SIDE as i8).contains(&file) && (0..BOARD_SIDE as i8).contains(&rank) {
            Self::new(file as u8, rank as u8)
        } else {
            None
        }
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", (b'a' + self.file()) as char, self.rank() + 1)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i8)]
pub enum PawnDirection {
    TowardWhite = -1,
    TowardBlack = 1,
}

impl PawnDirection {
    const fn rank_delta(self) -> i8 {
        self as i8
    }

    const fn reversed(self) -> Self {
        match self {
            Self::TowardWhite => Self::TowardBlack,
            Self::TowardBlack => Self::TowardWhite,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReturningPawnCapture {
    /// A pawn on its return trip cannot capture: the conservative alternate
    /// reading of "only capture when moving forward."
    OutboundOnly,
    /// Captures always point toward the opponent, even while the pawn's
    /// non-capturing move points homeward.
    TowardOpponent,
    /// Captures follow the pawn's current travel direction.
    TravelDirection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rules {
    pub returning_pawn_capture: ReturningPawnCapture,
}

impl Rules {
    pub const ORIGINAL_OUTBOUND_ONLY: Self = Self {
        returning_pawn_capture: ReturningPawnCapture::OutboundOnly,
    };

    pub const ORIGINAL_TOWARD_OPPONENT: Self = Self {
        returning_pawn_capture: ReturningPawnCapture::TowardOpponent,
    };

    pub const ORIGINAL_TRAVEL_DIRECTION: Self = Self {
        returning_pawn_capture: ReturningPawnCapture::TravelDirection,
    };
}

impl Default for Rules {
    fn default() -> Self {
        Self::ORIGINAL_TRAVEL_DIRECTION
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Move {
    Place { piece: PieceKind, to: Square },
    Move { from: Square, to: Square },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    board: [Option<Piece>; BOARD_CELLS],
    side_to_move: Color,
    pawn_directions: [PawnDirection; 2],
    opening_complete: bool,
}

impl Default for Position {
    fn default() -> Self {
        Self::initial()
    }
}

impl Position {
    pub const fn initial() -> Self {
        Self {
            board: [None; BOARD_CELLS],
            side_to_move: Color::White,
            pawn_directions: [PawnDirection::TowardBlack, PawnDirection::TowardWhite],
            opening_complete: false,
        }
    }

    pub const fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub const fn opening_complete(&self) -> bool {
        self.opening_complete
    }

    pub const fn at(&self, square: Square) -> Option<Piece> {
        self.board[square.index()]
    }

    pub const fn pawn_direction(&self, color: Color) -> PawnDirection {
        self.pawn_directions[color.index()]
    }

    pub fn winner(&self) -> Option<Color> {
        for line in WIN_LINES {
            let Some(first) = self.board[line[0]] else {
                continue;
            };
            if line[1..]
                .iter()
                .all(|&index| self.board[index].is_some_and(|piece| piece.color == first.color))
            {
                return Some(first.color);
            }
        }
        None
    }

    pub fn is_terminal(&self) -> bool {
        self.winner().is_some()
    }

    pub fn on_board_count(&self, color: Color) -> usize {
        self.board
            .iter()
            .flatten()
            .filter(|piece| piece.color == color)
            .count()
    }

    pub fn piece_square(&self, wanted: Piece) -> Option<Square> {
        self.board.iter().enumerate().find_map(|(index, piece)| {
            (*piece == Some(wanted)).then(|| Square::from_index(index as u8).unwrap())
        })
    }

    pub fn legal_moves(&self, rules: Rules) -> Vec<Move> {
        if self.is_terminal() {
            return Vec::new();
        }

        let mut moves = self.placement_moves();
        if !self.opening_complete {
            return moves;
        }

        for index in 0..BOARD_CELLS as u8 {
            let from = Square::from_index(index).unwrap();
            let Some(piece) = self.at(from) else {
                continue;
            };
            if piece.color != self.side_to_move {
                continue;
            }
            self.piece_moves(from, piece, rules, &mut moves);
        }
        moves
    }

    pub fn play(&self, action: Move, rules: Rules) -> Result<Self, IllegalMove> {
        if !self.legal_moves(rules).contains(&action) {
            return Err(IllegalMove(action));
        }

        Ok(self.play_unchecked(action))
    }

    pub(crate) fn play_unchecked(&self, action: Move) -> Self {
        let mover = self.side_to_move;
        let mut next = self.clone();
        match action {
            Move::Place { piece, to } => {
                let placed = Piece {
                    color: mover,
                    kind: piece,
                };
                next.board[to.index()] = Some(placed);
                if piece == PieceKind::Pawn {
                    next.pawn_directions[mover.index()] = initial_pawn_direction(mover);
                    next.normalize_pawn_edge(mover, to);
                }
            }
            Move::Move { from, to } => {
                let piece = next.board[from.index()].take().expect("legal source");
                if let Some(captured) = next.board[to.index()] {
                    if captured.kind == PieceKind::Pawn {
                        next.pawn_directions[captured.color.index()] =
                            initial_pawn_direction(captured.color);
                    }
                }
                next.board[to.index()] = Some(piece);
                if piece.kind == PieceKind::Pawn {
                    next.normalize_pawn_edge(mover, to);
                }
            }
        }

        if !next.opening_complete
            && Color::ALL
                .into_iter()
                .all(|color| next.on_board_count(color) >= 3)
        {
            next.opening_complete = true;
        }
        next.side_to_move = mover.opponent();
        next
    }

    fn placement_moves(&self) -> Vec<Move> {
        let mut moves = Vec::new();
        for kind in PieceKind::ALL {
            let piece = Piece {
                color: self.side_to_move,
                kind,
            };
            if self.piece_square(piece).is_some() {
                continue;
            }
            for index in 0..BOARD_CELLS as u8 {
                let to = Square::from_index(index).unwrap();
                if self.at(to).is_none() {
                    moves.push(Move::Place { piece: kind, to });
                }
            }
        }
        moves
    }

    fn piece_moves(&self, from: Square, piece: Piece, rules: Rules, moves: &mut Vec<Move>) {
        match piece.kind {
            PieceKind::Pawn => self.pawn_moves(from, piece, rules, moves),
            PieceKind::Knight => {
                for (df, dr) in [
                    (1, 2),
                    (2, 1),
                    (2, -1),
                    (1, -2),
                    (-1, -2),
                    (-2, -1),
                    (-2, 1),
                    (-1, 2),
                ] {
                    if let Some(to) = from.offset(df, dr) {
                        self.push_if_open_or_enemy(from, to, piece.color, moves);
                    }
                }
            }
            PieceKind::Bishop => {
                for direction in [(1, 1), (1, -1), (-1, -1), (-1, 1)] {
                    self.slide(from, piece.color, direction, moves);
                }
            }
            PieceKind::Rook => {
                for direction in [(1, 0), (0, -1), (-1, 0), (0, 1)] {
                    self.slide(from, piece.color, direction, moves);
                }
            }
        }
    }

    fn pawn_moves(&self, from: Square, piece: Piece, rules: Rules, moves: &mut Vec<Move>) {
        let travel = self.pawn_direction(piece.color).rank_delta();
        if let Some(to) = from.offset(0, travel) {
            if self.at(to).is_none() {
                moves.push(Move::Move { from, to });
            }
        }

        let initial = initial_pawn_direction(piece.color).rank_delta();
        let capture_delta = match rules.returning_pawn_capture {
            ReturningPawnCapture::OutboundOnly if travel != initial => return,
            ReturningPawnCapture::OutboundOnly | ReturningPawnCapture::TowardOpponent => initial,
            ReturningPawnCapture::TravelDirection => travel,
        };
        for file_delta in [-1, 1] {
            if let Some(to) = from.offset(file_delta, capture_delta) {
                if self
                    .at(to)
                    .is_some_and(|target| target.color != piece.color)
                {
                    moves.push(Move::Move { from, to });
                }
            }
        }
    }

    fn slide(
        &self,
        from: Square,
        color: Color,
        (file_delta, rank_delta): (i8, i8),
        moves: &mut Vec<Move>,
    ) {
        let mut cursor = from;
        while let Some(to) = cursor.offset(file_delta, rank_delta) {
            match self.at(to) {
                None => moves.push(Move::Move { from, to }),
                Some(piece) if piece.color != color => {
                    moves.push(Move::Move { from, to });
                    break;
                }
                Some(_) => break,
            }
            cursor = to;
        }
    }

    fn push_if_open_or_enemy(&self, from: Square, to: Square, color: Color, moves: &mut Vec<Move>) {
        if self.at(to).is_none_or(|piece| piece.color != color) {
            moves.push(Move::Move { from, to });
        }
    }

    fn normalize_pawn_edge(&mut self, color: Color, square: Square) {
        let direction = self.pawn_directions[color.index()];
        let points_off_board = match direction {
            PawnDirection::TowardWhite => square.rank() == 0,
            PawnDirection::TowardBlack => square.rank() == BOARD_SIDE - 1,
        };
        if points_off_board {
            self.pawn_directions[color.index()] = direction.reversed();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IllegalMove(pub Move);

impl fmt::Display for IllegalMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "illegal move: {:?}", self.0)
    }
}

impl std::error::Error for IllegalMove {}

const WIN_LINES: [[usize; 4]; 10] = [
    [0, 1, 2, 3],
    [4, 5, 6, 7],
    [8, 9, 10, 11],
    [12, 13, 14, 15],
    [0, 4, 8, 12],
    [1, 5, 9, 13],
    [2, 6, 10, 14],
    [3, 7, 11, 15],
    [0, 5, 10, 15],
    [3, 6, 9, 12],
];

const fn initial_pawn_direction(color: Color) -> PawnDirection {
    match color {
        Color::White => PawnDirection::TowardBlack,
        Color::Black => PawnDirection::TowardWhite,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(name: &str) -> Square {
        let bytes = name.as_bytes();
        Square::new(bytes[0] - b'a', bytes[1] - b'1').unwrap()
    }

    fn place(position: &Position, kind: PieceKind, to: &str) -> Position {
        position
            .play(
                Move::Place {
                    piece: kind,
                    to: sq(to),
                },
                Rules::default(),
            )
            .unwrap()
    }

    fn opened_position() -> Position {
        let mut p = Position::initial();
        p = place(&p, PieceKind::Rook, "a1");
        p = place(&p, PieceKind::Rook, "d4");
        p = place(&p, PieceKind::Bishop, "b1");
        p = place(&p, PieceKind::Bishop, "c3");
        p = place(&p, PieceKind::Knight, "c1");
        p = place(&p, PieceKind::Knight, "a4");
        p
    }

    #[test]
    fn initial_position_has_64_placements_and_no_moves() {
        let moves = Position::initial().legal_moves(Rules::default());
        assert_eq!(moves.len(), 64);
        assert!(moves
            .iter()
            .all(|action| matches!(action, Move::Place { .. })));
    }

    #[test]
    fn opening_unlocks_only_after_both_third_placements() {
        let mut p = Position::initial();
        p = place(&p, PieceKind::Rook, "a1");
        p = place(&p, PieceKind::Rook, "d4");
        p = place(&p, PieceKind::Bishop, "b1");
        p = place(&p, PieceKind::Bishop, "c4");
        p = place(&p, PieceKind::Knight, "c1");
        assert!(!p.opening_complete());
        assert!(p
            .legal_moves(Rules::default())
            .iter()
            .all(|action| matches!(action, Move::Place { .. })));

        p = place(&p, PieceKind::Knight, "b4");
        assert!(p.opening_complete());
        assert!(p.legal_moves(Rules::default()).contains(&Move::Move {
            from: sq("a1"),
            to: sq("a2")
        }));
    }

    #[test]
    fn capture_returns_piece_to_hand_for_later_placement() {
        let p = opened_position();
        let p = p
            .play(
                Move::Move {
                    from: sq("a1"),
                    to: sq("a4"),
                },
                Rules::default(),
            )
            .unwrap();
        assert_eq!(
            p.at(sq("a4")),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Rook
            })
        );
        assert!(p.legal_moves(Rules::default()).contains(&Move::Place {
            piece: PieceKind::Knight,
            to: sq("a1")
        }));
    }

    #[test]
    fn pawn_redeployment_resets_and_edge_placement_points_inward() {
        let mut p = opened_position();
        p = place(&p, PieceKind::Pawn, "b4");
        assert_eq!(p.pawn_direction(Color::White), PawnDirection::TowardWhite);
    }

    #[test]
    fn returning_pawn_capture_is_a_named_rule_variant() {
        let mut p = opened_position();
        p = place(&p, PieceKind::Pawn, "b4");
        p = place(&p, PieceKind::Pawn, "d2");

        // The black bishop already occupies c3. Only the travel-direction
        // reading lets the returning white pawn capture it.
        let backward_capture = Move::Move {
            from: sq("b4"),
            to: sq("c3"),
        };
        assert!(!p
            .legal_moves(Rules::ORIGINAL_OUTBOUND_ONLY)
            .contains(&backward_capture));
        assert!(p
            .legal_moves(Rules::ORIGINAL_TRAVEL_DIRECTION)
            .contains(&backward_capture));
        assert!(p.legal_moves(Rules::default()).contains(&backward_capture));
    }

    #[test]
    fn placing_four_in_a_row_wins_and_stops_move_generation() {
        let mut p = opened_position();
        p = place(&p, PieceKind::Pawn, "d1");
        assert_eq!(p.winner(), Some(Color::White));
        assert!(p.legal_moves(Rules::default()).is_empty());
    }

    #[test]
    fn winner_scans_past_empty_earlier_lines() {
        let mut p = Position::initial();
        for rank in 0..BOARD_SIDE {
            p.board[Square::new(3, rank).unwrap().index()] = Some(Piece {
                color: Color::Black,
                kind: PieceKind::Rook,
            });
        }
        assert_eq!(p.winner(), Some(Color::Black));
    }

    #[test]
    fn square_round_trip() {
        for index in 0..BOARD_CELLS as u8 {
            let square = Square::from_index(index).unwrap();
            assert_eq!(Square::new(square.file(), square.rank()), Some(square));
        }
    }
}
