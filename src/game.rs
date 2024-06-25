use crate::{
	bitboard::{cast_cardinals, cast_diagonals, Bb, KING_PATTERNS, KNIGHT_PATTERNS},
	state::{Board, Color, Move, Piece, PieceType, SpecialMove, Square}
};

pub enum GameResult {
	Checkmate(Color),
	Draw,
}

#[derive(Clone)]
pub struct Position {
	board: Board,
	unmoved: Bb, // for pawns (push), rooks and kings (castling)
	en_passant_target: Option<Square>,
	ply_number: u16,
	half_move_clock: u8,
}
impl Position {
	pub const FEN_INITIAL: &'static str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

	pub fn side_to_move(&self) -> Color {
		Color::from_ordinal(((self.ply_number - 1) % 2) as u8)
	}
	pub fn get_board(&self) -> &Board {
		&self.board
	}
	pub fn get_ply(&self) -> u16 {
		self.ply_number
	}

	pub fn from_fen(fen: &str) -> Option<Position> {
		let mut fields = fen.split(' ');

		let board = Board::from_fen(fields.next()?)?;
		
		let mut unmoved = Bb::EMPTY;
		unmoved |= board.find_piece(Piece::new(Color::White, PieceType::Pawn)) & Bb::rank(1);
		unmoved |= board.find_piece(Piece::new(Color::Black, PieceType::Pawn)) & Bb::rank(6);

		let side_to_move = match fields.next()? {
			"w" => Color::White,
			"b" => Color::Black,
			_ => return None,
		};

		let castling_rights = fields.next()?;
		if castling_rights != "-" {
			for c in castling_rights.chars() {
				let (color, rook_pos, king_pos) = match c {
					'K'|'Q' => (
						Color::White,
						Square::at(if c == 'Q' { 0 } else { 7 }, 0),
						Square::at(4, 0)
					),
					'k'|'q' => (
						Color::Black,
						Square::at(if c == 'q' { 0 } else { 7 }, 7),
						Square::at(4, 7)
					),
					_ => return None, // invalid syntax for castling rights
				};
				if !board.find_piece(Piece::new(color, PieceType::Rook)).at(rook_pos)
					|| !board.find_piece(Piece::new(color, PieceType::King)).at(king_pos) {
					// invalid castling rights: rook and/or king are not in expected position
					return None
				}
				unmoved |= Bb::one(rook_pos) | Bb::one(king_pos);
			}
		}

		let en_passant_target = fields.next()?;
		let en_passant_target = if en_passant_target == "-" {
			None
		} else {
			Some(Square::parse(en_passant_target)?)
		};

		let half_move_clock: u8 = fields.next()?.parse().ok()?;
		let move_number: u16 = fields.next()?.parse().ok()?;
		let ply_number = 2*move_number + side_to_move as u16 - 1;
		if fields.next().is_some() {
			return None
		}

		Some(Position { board, unmoved, en_passant_target, ply_number, half_move_clock })
	}

	#[cfg(test)]
	pub fn to_fen(&self) -> String {
		use std::fmt::Write;

		let mut res = self.board.to_fen();
		write!(res, " {} ", self.side_to_move().to_fen()).unwrap();

		let kw = self.unmoved.at(Square::at(4,0));
		let kb = self.unmoved.at(Square::at(4,7));
		let ckw = kw && self.unmoved.at(Square::at(7,0));
		let cqw = kw && self.unmoved.at(Square::at(0,0));
		let ckb = kb && self.unmoved.at(Square::at(7,7));
		let cqb = kb && self.unmoved.at(Square::at(0,7));
		if !ckw && !cqw && !ckb && !cqb {
			res.push('-');
		} else {
			if ckw { res.push('K') }
			if cqw { res.push('Q') }
			if ckb { res.push('k') }
			if cqb { res.push('q') }
		}
		res.push(' ');

		if let Some(squ) = self.en_passant_target {
			write!(res, "{}", squ).unwrap();
		} else {
			res.push('-');
		}

		let move_number = (self.ply_number - 1) / 2 + 1;
		write!(res, " {} {}", self.half_move_clock, move_number).unwrap();

		res
	}

	pub fn apply_move(&mut self, mov: &Move) {
		let color = self.side_to_move();
		debug_assert!(self.board.find_piece(Piece::new(color, mov.ptype)).at(mov.from),
			"invalid move: expected piece not found on source square");
		let own_pieces = self.board.find_color(color);
		debug_assert!(!own_pieces.at(mov.to), "invalid move: own piece on target square");

		// deal with captures and special moves
		let mut capture = false;
		match mov.special {
			SpecialMove::EnPassant => {
				debug_assert!(mov.ptype == PieceType::Pawn, "invalid en passant: not a pawn");
				debug_assert!(mov.from.rank() == color.rel_rank(4), "invalid en passant: invalid source");
				debug_assert!(self.en_passant_target == Some(mov.to), "invalid en passant: invalid target");
				let piece = Piece::new(color.opponent(), PieceType::Pawn);
				let pawn_squ = Square::at(mov.to.file(), mov.from.rank());
				debug_assert!(self.board.find_piece(piece).at(pawn_squ),
					"invalid en passant: enemy pawn not found");
				self.board.remove(pawn_squ, piece);
				self.unmoved &= !Bb::one(mov.to);
				capture = true;
			},
			SpecialMove::CastleQ | SpecialMove::CastleK => {
				debug_assert!(mov.ptype == PieceType::King, "invalid castling: not a king");
				debug_assert!(mov.from.rank() == color.rel_rank(0) && mov.from.file() == 4,
					"invalid castling: king not in initial position");
				debug_assert!(self.unmoved.at(mov.from), "invalid castling: king was moved");
				let dfile = mov.to.file() as i8 - mov.from.file() as i8;
				debug_assert!(mov.from.rank() == mov.to.rank() && dfile.abs() == 2,
					"invalid castling: wrong move pattern");
				let rank = mov.from.rank();
				let middle_squ = Square::at(mov.from.file().checked_add_signed(dfile/2).unwrap(), rank);
				let corner_squ = Square::at(if dfile > 0 { 7 } else { 0 }, rank);
				let rook_piece = Piece::new(color, PieceType::Rook);
				debug_assert!(self.board.find_piece(rook_piece).at(corner_squ),
					"invalid castling: rook not found");
				debug_assert!(self.unmoved.at(corner_squ), "invalid castling: rook was moved");
				debug_assert!(!self.board.all_pieces().at(middle_squ), "invalid castling: piece in the way");
				self.board.remove(corner_squ, rook_piece);
				self.unmoved &= !Bb::one(corner_squ);
				self.board.add(middle_squ, rook_piece);
			},
			_ => {
				for ptype in PieceType::all() {
					let piece = Piece::new(color.opponent(), ptype);
					let bb = self.board.find_piece(piece);
					if bb.at(mov.to) { // capture
						self.board.remove(mov.to, piece);
						self.unmoved &= !Bb::one(mov.to);
						capture = true;
					}
				}
			},
		}
		if mov.ptype == PieceType::Pawn && self.unmoved.at(mov.from) && mov.from.file() == mov.to.file()
			&& mov.to.rank().abs_diff(mov.from.rank()) == 2 {
			self.en_passant_target = Some(Square::at(mov.from.file(), (mov.from.rank() + mov.to.rank())/2));
		} else {
			self.en_passant_target = None;
		}

		// move piece
		let mut my_piece = Piece::new(color, mov.ptype);
		self.board.remove(mov.from, my_piece);
		if let Some(promotion) = mov.special.get_promotion() {
			assert!(my_piece.ptype == PieceType::Pawn && mov.to.rank() == color.rel_rank(7), "invalid promotion");
			my_piece.ptype = promotion;
		}
		self.board.add(mov.to, my_piece);

		self.unmoved &= !(Bb::one(mov.from) | Bb::one(mov.to));
		self.ply_number += 1;
		if !capture && mov.ptype != PieceType::Pawn {
			self.half_move_clock += 1;
		} else {
			self.half_move_clock = 0;
		}
	}

	fn find_king(&self, color: Color) -> Option<Square> {
		let bb = self.board.find_piece(Piece::new(color, PieceType::King));
		assert!(bb.count() <= 1, "more than 1 king of the same color on board");
		bb.iter().next()
	}

	fn gen_attacked(&self, color: Color, pieces: Bb) -> Bb {
		let mut attacked = Bb::EMPTY;
		let pawn_forward = self.board.find_piece(Piece::new(color, PieceType::Pawn)).shift_ver(color.up());
		attacked |= pawn_forward.shift_left(1) | pawn_forward.shift_right(1);
		for from in self.board.find_piece(Piece::new(color, PieceType::Knight)).iter() {
			attacked |= KNIGHT_PATTERNS[from];
		}
		for from in self.board.find_piece(Piece::new(color, PieceType::Bishop)).iter() {
			attacked |= cast_diagonals(from, pieces);
		}
		for from in self.board.find_piece(Piece::new(color, PieceType::Rook)).iter() {
			attacked |= cast_cardinals(from, pieces);
		}
		for from in self.board.find_piece(Piece::new(color, PieceType::Queen)).iter() {
			attacked |= cast_cardinals(from, pieces) | cast_diagonals(from, pieces);
		}
		if let Some(king_pos) = self.find_king(color) {
			attacked |= KING_PATTERNS[king_pos];
		}
		return attacked;
	}

	fn gen_pawn_moves(out: &mut Vec<Move>, color: Color, from: Square, to: Square) {
		let specials: &[SpecialMove] = if to.rank() == color.rel_rank(7) {
			&[SpecialMove::PromoteN, SpecialMove::PromoteB, SpecialMove::PromoteR, SpecialMove::PromoteQ]
		} else {
			&[SpecialMove::None]
		};
		for special in specials {
			out.push(Move {
				ptype: PieceType::Pawn, special: *special,
				from, to,
			})
		}
	}

	pub fn gen_pseudolegal(&self) -> Vec<Move> {
		let mut moves = Vec::with_capacity(256);

		let color = self.side_to_move();
		let allies = self.board.find_color(color);
		let enemies = self.board.find_color(color.opponent());
		let pieces = allies | enemies;

		// pawns

		let pawns = self.board.find_piece(Piece::new(color, PieceType::Pawn));
		let mut pawn_forward = pawns.shift_ver(color.up());
		let pawn_cap_left = pawn_forward.shift_left(1);
		let pawn_cap_right = pawn_forward.shift_right(1);
		if color == self.side_to_move() && self.en_passant_target.is_some() {
			let squ = self.en_passant_target.unwrap();
			if pawn_cap_left.at(squ) {
				moves.push(Move {
					ptype: PieceType::Pawn, special: SpecialMove::EnPassant,
					from: squ.shift(1, color.down()), to: squ,
				})
			}
			if pawn_cap_right.at(squ) {
				moves.push(Move {
					ptype: PieceType::Pawn, special: SpecialMove::EnPassant,
					from: squ.shift(-1, color.down()), to: squ,
				})
			}
		}
		pawn_forward &= !pieces;
		let pawn_push = pawn_forward.shift_ver(color.up()) & !pieces & self.unmoved.shift_up(2);
		for to in pawn_forward.iter() {
			Position::gen_pawn_moves(&mut moves, color, to.shift(0, color.down()), to);
		}
		for to in pawn_push.iter() {
			Position::gen_pawn_moves(&mut moves, color, to.shift(0, color.down() * 2), to);
		}
		for to in (pawn_cap_left & enemies).iter() {
			Position::gen_pawn_moves(&mut moves, color, to.shift(1, color.down()), to);
		}
		for to in (pawn_cap_right & enemies).iter() {
			Position::gen_pawn_moves(&mut moves, color, to.shift(-1, color.down()), to);
		}

		// knights

		let knights = self.board.find_piece(Piece::new(color, PieceType::Knight));
		for from in knights.iter() {
			for to in (KNIGHT_PATTERNS[from] & !allies).iter() {
				moves.push(Move {
					ptype: PieceType::Knight, special: SpecialMove::None,
					from, to,
				})
			}
		}

		// bishops

		let bishops = self.board.find_piece(Piece::new(color, PieceType::Bishop));
		for from in bishops.iter() {
			for to in cast_diagonals(from, pieces).iter() {
				if !allies.at(to) {
					moves.push(Move {
						ptype: PieceType::Bishop, special: SpecialMove::None,
						from, to,
					})
				}
			}
		}

		// rooks

		let rooks = self.board.find_piece(Piece::new(color, PieceType::Rook));
		for from in rooks.iter() {
			for to in cast_cardinals(from, pieces).iter() {
				if !allies.at(to) {
					moves.push(Move {
						ptype: PieceType::Rook, special: SpecialMove::None,
						from, to,
					})
				}
			}
		}

		// queens

		let queens = self.board.find_piece(Piece::new(color, PieceType::Queen));
		for from in queens.iter() {
			for to in (cast_cardinals(from, pieces) | cast_diagonals(from, pieces)).iter() {
				if !allies.at(to) {
					moves.push(Move {
						ptype: PieceType::Queen, special: SpecialMove::None,
						from, to,
					})
				}
			}
		}

		// kings

		if let Some(king_pos) = self.find_king(color) {
			let attacked = self.gen_attacked(color.opponent(), pieces);
			for to in (KING_PATTERNS[king_pos] & !allies).iter() {
				moves.push(Move {
					ptype: PieceType::King, special: SpecialMove::None,
					from: king_pos, to,
				})
			}
			if self.unmoved.at(king_pos) {
				let rank0 = color.rel_rank(0);
				debug_assert!(king_pos.rank() == rank0 && king_pos.file() == 4);
				let queen_corner = Square::at(0, rank0);
				let king_corner = Square::at(7, rank0);
				let queen_area = Bb(0x0000000101010000).shift_up(rank0);
				let king_area = Bb(0x0001010100000000).shift_up(rank0);
				let except_king = pieces & !Bb::one(king_pos);
				let queen_side = self.unmoved.at(queen_corner) && (queen_area & except_king).none();
				let king_side = self.unmoved.at(king_corner) && (king_area & except_king).none();
				if queen_side || king_side {
					if queen_side && (attacked & queen_area).none() {
						moves.push(Move {
							ptype: PieceType::King, special: SpecialMove::CastleQ,
							from: king_pos, to: Square::at(2, rank0),
						});
					}
					if king_side && (attacked & king_area).none() {
						moves.push(Move {
							ptype: PieceType::King, special: SpecialMove::CastleK,
							from: king_pos, to: Square::at(6, rank0),
						});
					}
				}
			}
		}

		moves
	}

	pub fn is_in_check(&self, color: Color) -> bool {
		if let Some(king_pos) = self.find_king(color) {
			self.gen_attacked(color.opponent(), self.board.all_pieces()).at(king_pos)
		} else {
			true // in the hypothetical that the king was captured
		}
	}

	pub fn gen_legal(&self) -> Vec<Move> {
		if self.half_move_clock >= 75 {
			return vec![]; // draw
		}
		let color = self.side_to_move();
		let mut moves = self.gen_pseudolegal();
		moves.retain(|mov| {
			let mut pos = self.clone();
			pos.apply_move(mov);
			!pos.is_in_check(color)
		});
		moves
	}
}

#[cfg(test)]
mod test_movegen {
	use serde::Deserialize;

use crate::{game::Position, state::{Move, ParseMoveError}};

	#[derive(Deserialize)]
	#[serde(rename_all = "camelCase")]
	struct TestFile {
		description: String,
		test_cases: Vec<TestCase>,
	}

	#[derive(Deserialize)]
	struct TestCase {
		start: TestStart,
		expected: Vec<TestMove>,
	}

	#[derive(Deserialize)]
	struct TestStart {
		description: Option<String>,
		fen: String,
	}

	#[derive(Deserialize)]
	struct TestMove {
		r#move: String,
		fen: String,
	}

	fn run_test_file(json: &str) {
		let file: TestFile = serde_json::from_str(json).unwrap();
		println!("Description: {}", file.description);
		let mut failures = 0;
		for (i, case) in file.test_cases.into_iter().enumerate() {
			if let Some(desc) = case.start.description {
				println!("Test #{}: {}", i, desc);
			} else {
				println!("Test #{}:", i);
			}
			let pos = Position::from_fen(&case.start.fen).expect("Invalid FEN");
			println!("FEN: {}", case.start.fen);
			println!("{}", pos.board);

			let moves = pos.gen_legal();
			for mov in moves.iter() {
				let mut pos2 = pos.clone();
				pos2.apply_move(&mov);
				let fen_after = pos2.to_fen();
				if case.expected.iter().all(|m| m.fen != fen_after) {
					println!("(!) Our move {} -> FEN {} is unexpected", mov, fen_after);
					failures += 1;
				}
			}

			for mov in case.expected {
				if let Err(err) = Move::parse(&mov.r#move, &moves) {
					let err_desc = match err {
						ParseMoveError::AmbiguousMove => "ambiguous",
						ParseMoveError::IllegalMove => "illegal",
						ParseMoveError::InvalidSyntax => "invalid syntax",
					};
					println!("(!) Their move {} -> FEN {} is deemed {}", mov.r#move, mov.fen, err_desc);
					failures += 1;
				}
			}
		}

		assert!(failures == 0, "{} failures occured!", failures);
	}

	#[test]
	fn test_standard() {
		run_test_file(include_str!("../tests/standard.json"));
	}
	#[test]
	fn test_famous() {
		run_test_file(include_str!("../tests/famous.json"));
	}
	#[test]
	fn test_pawns() {
		run_test_file(include_str!("../tests/pawns.json"));
	}
	#[test]
	fn test_promotions() {
		run_test_file(include_str!("../tests/promotions.json"));
	}
	#[test]
	fn test_castling() {
		run_test_file(include_str!("../tests/castling.json"));
	}
	#[test]
	fn test_taxing() {
		run_test_file(include_str!("../tests/taxing.json"));
	}
}
