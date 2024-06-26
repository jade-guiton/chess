use std::fmt::{self, Write};

use crate::bitboard::Bb;

fn parse_file(c: u8) -> Option<u8> {
	if b'a' <= c && c <= b'h' {
		Some(c - b'a')
	} else {
		None
	}
}
fn parse_rank(c: u8) -> Option<u8> {
	if b'1' <= c && c <= b'8' {
		Some(c - b'1')
	} else {
		None
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Square { pub(crate) idx: u8 }
impl Square {
	pub fn at(file: u8, rank: u8) -> Square {
		debug_assert!(file < 8 && rank < 8);
		Square { idx: file << 3 | rank }
	}
	pub const fn file(self) -> u8 {
		self.idx >> 3
	}
	pub const fn rank(self) -> u8 {
		self.idx & 7
	}
	pub fn parse(s: &str) -> Option<Square> {
		let b = s.as_bytes();
		if b.len() == 2 {
			Some(Square::at(parse_file(b[0])?, parse_rank(b[1])?))
		} else {
			None
		}
	}
	pub(crate) fn shift(self, dfile: i8, drank: i8) -> Square {
		let file = (self.file() as i8 + dfile) as u8;
		let rank = (self.rank() as i8 + drank) as u8;
		debug_assert!(file < 8 && rank < 8);
		Square::at(file, rank)
	}
}
impl<T> std::ops::Index<Square> for [T; 64] {
	type Output = T;
	fn index(&self, index: Square) -> &Self::Output {
		return &self[index.idx as usize];
	}
}
impl<T> std::ops::IndexMut<Square> for [T; 64] {
	fn index_mut(&mut self, index: Square) -> &mut Self::Output {
		return &mut self[index.idx as usize];
	}
}
impl fmt::Display for Square {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}{}",
			('a' as u8 + self.file()) as char,
			('1' as u8 + self.rank()) as char
		)
	}
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceType {
	Pawn,
	Knight,
	Bishop,
	Rook,
	Queen,
	King,
}
impl PieceType {
	pub fn algebraic(self) -> &'static str {
		["","N","B","R","Q","K"][self as usize]
	}
	fn from_ordinal(n: u8) -> PieceType {
		debug_assert!(n < 6);
		unsafe { std::mem::transmute(n) }
	}
	pub fn all() -> impl Iterator<Item=PieceType> {
		(0..6u8).map(PieceType::from_ordinal)
	}
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
	White,
	Black,
}
impl Color {
	pub fn opponent(self) -> Color {
		match self {
			Color::White => Color::Black,
			Color::Black => Color::White,
		}
	}
	pub fn to_fen(self) -> char {
		['w','b'][self as usize]
	}
	pub(crate) fn from_ordinal(n: u8) -> Color {
		debug_assert!(n < 2);
		unsafe { std::mem::transmute(n) }
	}
	fn all() -> impl Iterator<Item=Color> {
		(0..2u8).map(Color::from_ordinal)
	}
	pub fn rel_rank(self, rank: u8) -> u8 {
		match self {
			Color::White => rank,
			Color::Black => 7 - rank,
		}
	}
	pub fn up(self) -> i8 {
		match self {
			Color::White => 1,
			Color::Black => -1,
		}
	}
	pub fn down(self) -> i8 {
		-self.up()
	}
}
impl<T> std::ops::Index<Color> for [T; 2] {
	type Output = T;
	fn index(&self, index: Color) -> &Self::Output {
		return &self[index as usize];
	}
}
impl<T> std::ops::IndexMut<Color> for [T; 2] {
	fn index_mut(&mut self, index: Color) -> &mut Self::Output {
		return &mut self[index as usize];
	}
}
impl fmt::Display for Color {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", match self {
			Color::White => "White",
			Color::Black => "Black",
		})
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
	pub color: Color,
	pub ptype: PieceType,
}
impl Piece {
	pub fn new(color: Color, ptype: PieceType) -> Piece {
		Piece { color, ptype }
	}
	fn ordinal(self) -> usize {
		(self.color as usize) * 6 + self.ptype as usize
	}
	const FEN_NOTATION: &'static[u8] = b"PNBRQKpnbrqk";
	pub fn to_fen(self) -> u8 {
		Piece::FEN_NOTATION[self.ordinal()]
	}
	pub fn from_fen(c: u8) -> Option<Piece> {
		Piece::FEN_NOTATION.iter().position(|c2| *c2 == c).map(|ord|
			Piece::new(Color::from_ordinal(ord as u8 / 6), PieceType::from_ordinal(ord as u8 % 6)))
	}
}
impl<T> std::ops::Index<Piece> for [T; 12] {
	type Output = T;
	fn index(&self, index: Piece) -> &Self::Output {
		return &self[index.ordinal()];
	}
}
impl<T> std::ops::IndexMut<Piece> for [T; 12] {
	fn index_mut(&mut self, index: Piece) -> &mut Self::Output {
		return &mut self[index.ordinal()];
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpecialMove {
	None,
	EnPassant,
	PromoteN,
	PromoteB,
	PromoteR,
	PromoteQ,
	CastleQ,
	CastleK,
}
impl SpecialMove {
	pub fn get_promotion(self) -> Option<PieceType> {
		match self {
			SpecialMove::PromoteN => Some(PieceType::Knight),
			SpecialMove::PromoteB => Some(PieceType::Bishop),
			SpecialMove::PromoteR => Some(PieceType::Rook),
			SpecialMove::PromoteQ => Some(PieceType::Queen),
			_ => None,
		}
	}
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct Move {
	pub ptype: PieceType,
	pub from: Square,
	pub to: Square,
	pub special: SpecialMove,
}

#[cfg(test)]
pub enum ParseMoveError {
	InvalidSyntax,
	AmbiguousMove,
	IllegalMove,
}
#[cfg(test)]
fn or_invalid<T>(opt: Option<T>) -> Result<T, ParseMoveError> {
	opt.ok_or(ParseMoveError::InvalidSyntax)
}
#[cfg(test)]
impl Move {
	pub fn parse<'moves>(s: &str, legal_moves: &'moves [Move]) -> Result<&'moves Move, ParseMoveError> {
		if let Some(special_move) = match s {
			"O-O-O" | "0-0-0" => Some(SpecialMove::CastleQ),
			"O-O" | "0-0" => Some(SpecialMove::CastleK),
			_ => None,
		} {
			let mut mov = None;
			for mov2 in legal_moves {
				if mov2.special == special_move {
					debug_assert!(mov.is_none(), "redundant castlings in legal move list");
					mov = Some(mov2);
				}
			}
			return mov.ok_or(ParseMoveError::IllegalMove);
		}

		let mut chars = s.chars().peekable();

		let ptype = match or_invalid(chars.peek())? {
			'N' => PieceType::Knight,
			'B' => PieceType::Bishop,
			'R' => PieceType::Rook,
			'Q' => PieceType::Queen,
			'K' => PieceType::King,
			_ => PieceType::Pawn,
		};
		if ptype != PieceType::Pawn {
			chars.next();
		}

		let mut file1 = parse_file(*or_invalid(chars.peek())? as u8);
		if file1.is_some() { chars.next(); }
		let mut rank1 = parse_rank(*or_invalid(chars.peek())? as u8);
		if rank1.is_some() { chars.next(); }

		// ignore capture flag
		if chars.peek() == Some(&'x') {
			chars.next();
		}

		let mut file2 = chars.peek().and_then(|c| parse_file(*c as u8));
		if file2.is_some() { chars.next(); }
		let mut rank2 = chars.peek().and_then(|c| parse_rank(*c as u8));
		if rank2.is_some() { chars.next(); }

		if file2.is_none() && rank2.is_none() {
			file2 = file1.take();
			rank2 = rank1.take();
		}

		let to_squ = Square::at(or_invalid(file2)?, or_invalid(rank2)?);
		
		let equal = if chars.peek() == Some(&'=') {
			chars.next();
			true
		} else {
			false
		};
		let promotion = match chars.peek() {
			Some('N') => Some(PieceType::Knight),
			Some('B') => Some(PieceType::Bishop),
			Some('R') => Some(PieceType::Rook),
			Some('Q') => Some(PieceType::Queen),
			_ => None,
		};
		if promotion.is_some() {
			chars.next();
		} else if equal {
			return Err(ParseMoveError::InvalidSyntax);
		}
		
		// ignore check/checkmate flags
		if chars.peek() == Some(&'+') {
			chars.next();
		}
		if chars.peek() == Some(&'#') {
			chars.next();
		}

		let mut mov = None;
		for mov2 in legal_moves {
			if mov2.ptype == ptype && mov2.to == to_squ
				&& (file1.is_none() || file1.unwrap() == mov2.from.file())
				&& (rank1.is_none() || rank1.unwrap() == mov2.from.rank())
				&& promotion == mov2.special.get_promotion() {
				if mov.is_some() {
					return Err(ParseMoveError::AmbiguousMove);
				}
				mov = Some(mov2)
			}
		}
		mov.ok_or(ParseMoveError::IllegalMove)
	}
}

impl Move {
	pub fn uci_notation(&self) -> String {
		let mut res = format!("{}{}", self.from, self.to);
		if let Some(promote_to) = self.special.get_promotion() {
			write!(res, "{}", promote_to.algebraic()).unwrap();
		}
		res
	}
}

impl fmt::Display for Move {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}{}{}", self.ptype.algebraic(), self.from, self.to)?;
		if let Some(promote_to) = self.special.get_promotion() {
			write!(f, "{}", promote_to.algebraic())?;
		}
		Ok(())
	}
}

#[derive(Default, Clone)]
pub struct Board([Bb; 12]); // bitboard for each piece
impl Board {
	pub fn find_piece(&self, piece: Piece) -> Bb {
		self.0[piece]
	}
	pub fn find_color(&self, color: Color) -> Bb {
		let mut sum = Bb::EMPTY;
		for ptype in PieceType::all() {
			sum |= self.0[Piece::new(color, ptype)];
		}
		sum
	}
	pub fn count_pieces(&self, color: Color, ptype: PieceType) -> u32 {
		self.find_piece(Piece::new(color, ptype)).count()
	}
	pub fn all_pieces(&self) -> Bb {
		self.find_color(Color::White) | self.find_color(Color::Black)
	}
	pub fn add(&mut self, squ: Square, piece: Piece) {
		self.0[piece] |= Bb::one(squ);
	}
	pub fn remove(&mut self, squ: Square, piece: Piece) {
		self.0[piece] &= !Bb::one(squ);
	}

	pub fn get_pieces(&self) -> [Option<Piece>; 64] {
		let mut board = [None; 64];
		for color in Color::all() {
			for ptype in PieceType::all() {
				for squ in self.0[Piece::new(color, ptype)].iter() {
					debug_assert!(board[squ].is_none(), "multiple piece types on same square");
					board[squ] = Some(Piece::new(color, ptype));
				}
			}
		}
		board
	}

	#[cfg(test)]
	pub fn to_fen(&self) -> String {
		let pieces = self.get_pieces();
		let mut res = String::new();
		for rank in (0..8).rev() {
			let mut blanks = 0;
			for file in 0..8 {
				let squ = Square::at(file, rank);
				if let Some(piece) = pieces[squ] {
					if blanks > 0 {
						res.push((b'0' + blanks as u8) as char);
						blanks = 0
					}
					res.push(piece.to_fen() as char);
				} else {
					blanks += 1;
				}
			}
			if blanks > 0 {
				res.push((b'0' + blanks as u8) as char);
			}
			if rank != 0 {
				res.push('/');
			}
		}
		res
	}

	pub fn from_fen(s: &str) -> Option<Board> {
		let mut board = Board::default();
		let mut rank = 8;
		for rank_field in s.split('/') {
			if rank == 0 {
				return None
			}
			rank -= 1;
			let mut file = 0;
			for c in rank_field.chars() {
				if file >= 8 || !c.is_ascii() {
					return None
				}
				if '1' <= c && c <= '8' {
					file += c as u8 - b'0';
				} else {
					let squ = Square::at(file, rank);
					let piece = Piece::from_fen(c as u8)?;
					board.add(squ, piece);
					file += 1;
				}
			}
			if file != 8 {
				return None
			}
		}
		if rank != 0 {
			return None
		}
		Some(board)
	}
}
impl fmt::Display for Board {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let pieces = self.get_pieces();
		for rank in (0..8).rev() {
			f.write_char('|')?;
			for file in 0..8 {
				let squ = Square::at(file, rank);
				if let Some(piece) = pieces[squ] {
					f.write_char(piece.to_fen() as char)?;
				} else {
					f.write_char(' ')?;
				}
			}
			f.write_char('|')?;
			if rank != 0 {
				f.write_char('\n')?;
			}
		}
		Ok(())
	}
}
