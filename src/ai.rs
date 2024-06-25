use std::{cell::RefCell, sync::{Arc, Mutex}, thread::JoinHandle, time::Instant};

use crate::{game::Position, state::{Board, Color, Move, Piece, PieceType, Square}};

pub trait ChessAi: Send {
	fn name(&self) -> String;
	fn pick_move(&mut self, pos: &Position, legal_moves: &[Move]) -> Move;
}

pub struct ParallelAi {
	ai: Arc<Mutex<Box<dyn ChessAi>>>,
	thinker: Option<JoinHandle<Move>>,
	name: RefCell<String>,
}
impl ParallelAi {
	pub fn new(ai: impl ChessAi + 'static) -> Self {
		ParallelAi {
			name: RefCell::new(ai.name()),
			ai: Arc::new(Mutex::new(Box::new(ai))),
			thinker: None,
		}
	}
	pub fn name(&self) -> String {
		if let Ok(ai) = self.ai.try_lock() {
			self.name.replace(ai.name());
		}
		self.name.borrow().clone()
	}
	pub fn pick_move_async(&mut self, pos: &Position, legal_moves: &[Move]) {
		let pos = pos.clone();
		let legal_moves = legal_moves.to_owned();
		let ai = self.ai.clone();
		self.thinker = Some(std::thread::spawn(move || {
			let mut ai = ai.lock().unwrap();
			return ai.pick_move(&pos, &legal_moves);
		}));
	}
	pub fn is_thinking(&self) -> bool {
		self.thinker.is_some()
	}
	pub fn try_get_result(&mut self) -> Option<Move> {
		if self.thinker.as_ref().expect("no active thinker thread").is_finished() {
			Some(self.thinker.take().unwrap().join().unwrap())
		} else {
			None
		}
	}
}

pub struct RandomAi();
impl ChessAi for RandomAi {
	fn name(&self) -> String {
		return "RandomAI".to_string();
	}
	fn pick_move(&mut self, _position: &Position, legal_moves: &[Move]) -> Move {
		legal_moves[rand::random::<usize>() % legal_moves.len()]
	}
}


// values adapted from:
// https://www.chessprogramming.org/Simplified_Evaluation_Function

const PAWN_VALUE: [i8; 64] = [
  0,  1,  1,  0,  1,  2, 10,  0,
  0,  2, -1,  0,  1,  2, 10,  0,
  0,  2, -2,  0,  2,  4, 10,  0,
  0, -4,  0,  4,  5,  6, 10,  0,
  0, -4,  0,  4,  5,  6, 10,  0,
  0,  2, -2,  0,  2,  4, 10,  0,
  0,  2, -1,  0,  1,  2, 10,  0,
  0,  1,  1,  0,  1,  2, 10,  0,
];
const KNIGHT_VALUE: [i8; 64] = [
-10, -8, -6, -6, -6, -6, -8,-10,
 -8, -4,  1,  0,  1,  0, -4, -8,
 -6,  0,  2,  3,  3,  2,  0, -6,
 -6,  1,  3,  4,  4,  3,  0, -6,
 -6,  1,  3,  4,  4,  3,  0, -6,
 -6,  0,  2,  3,  3,  2,  0, -6,
 -8, -4,  1,  0,  1,  0, -4, -8,
-10, -8, -6, -6, -6, -6, -8,-10,
];
const BISHOP_VALUE: [i8; 64] = [
 -4, -2, -2, -2, -2, -2, -2, -4,
 -2,  1,  2,  0,  1,  0,  0, -2,
 -2,  0,  2,  2,  1,  1,  0, -2,
 -2,  0,  2,  2,  2,  2,  0, -2,
 -2,  0,  2,  2,  2,  2,  0, -2,
 -2,  0,  2,  2,  1,  1,  0, -2,
 -2,  1,  2,  0,  1,  0,  0, -2,
 -4, -2, -2, -2, -2, -2, -2, -4,
];
const ROOK_VALUE: [i8; 64] = [
  0, -1, -1, -1, -1, -1,  1,  0,
  0,  0,  0,  0,  0,  0,  2,  0,
  0,  0,  0,  0,  0,  0,  2,  0,
  1,  0,  0,  0,  0,  0,  2,  0,
  1,  0,  0,  0,  0,  0,  2,  0,
  0,  0,  0,  0,  0,  0,  2,  0,
  0,  0,  0,  0,  0,  0,  2,  0,
  0, -1, -1, -1, -1, -1,  1,  0,
];
const QUEEN_VALUE: [i8; 64] = [
 -4, -2, -2,  0, -1, -2, -2, -4,
 -2,  0,  1,  0,  0,  0,  0, -2,
 -2,  1,  1,  1,  1,  1,  0, -2,
 -1,  0,  1,  1,  1,  1,  0, -1,
 -1,  0,  1,  1,  1,  1,  0, -1,
 -2,  0,  1,  1,  1,  1,  0, -2,
 -2,  0,  0,  0,  0,  0,  0, -2,
 -4, -2, -2, -1, -1, -2, -2, -4,
];
const KING_VALUE: [i8; 64] = [
  4,  4, -2, -4, -6, -6, -6, -6,
  6,  4, -4, -6, -8, -8, -8, -8,
  2,  0, -4, -6, -8, -8, -8, -8,
  0,  0, -4, -8,-10,-10,-10,-10,
  0,  0, -4, -8,-10,-10,-10,-10,
  2,  0, -4, -6, -8, -8, -8, -8,
  6,  4, -4, -6, -8, -8, -8, -8,
  4,  4, -2, -4, -6, -6, -6, -6,
];
const KING_VALUE_ENDGAME: [i8; 64] = [
-10, -6, -6, -6, -6, -6, -6,-10,
 -6, -6, -2, -2, -2, -2, -4, -8,
 -6,  0,  4,  6,  6,  4, -2, -6,
 -6,  0,  6,  8,  8,  6,  0, -4,
 -6,  0,  6,  8,  8,  6,  0, -4,
 -6,  0,  4,  6,  6,  4, -2, -6,
 -6, -6, -2, -2, -2, -2, -4, -8,
-10, -6, -6, -6, -6, -6, -6,-10,
];

fn eval_material(board: &Board, piece: Piece, base_val: i16, table: [i8; 64]) -> i16 {
	let bb = board.find_piece(piece);
	let mut val = 0;
	for mut squ in bb.iter() {
		// tables are from white's perspective, so flip the board if we're black
		if piece.color == Color::Black {
			squ = Square::at(squ.file(), 7 - squ.rank());
		}
		val += base_val + 5 * table[squ] as i16;
	}
	val
}

fn eval_side(board: &Board, color: Color, is_endgame: bool) -> i16 {
	let piece_data = [
		(PieceType::Pawn,   100,   PAWN_VALUE),
		(PieceType::Knight, 320,   KNIGHT_VALUE),
		(PieceType::Bishop, 330,   BISHOP_VALUE),
		(PieceType::Rook,   500,   ROOK_VALUE),
		(PieceType::Queen,  900,   QUEEN_VALUE),
		(PieceType::King,   20000, if is_endgame { KING_VALUE_ENDGAME } else { KING_VALUE }),
	];
	let mut val = 0;
	for (ptype, base_val, table) in piece_data {
		val += eval_material(board, Piece::new(color, ptype), base_val, table);
	}
	val
}

fn is_endgame(board: &Board, color: Color) -> bool {
	let queens = board.count_pieces(color, PieceType::Queen);
	let minor = board.count_pieces(color, PieceType::Knight)
		+ board.count_pieces(color, PieceType::Bishop);
	let other = board.count_pieces(color, PieceType::Pawn)
		+ board.count_pieces(color, PieceType::Rook);
	return queens == 0 || (minor <= 1 && other  == 0);
}

fn eval(board: &Board, color: Color) -> i16 {
	let is_endgame = is_endgame(board, color);
	eval_side(board, color, is_endgame) - eval_side(board, color.opponent(), is_endgame)
}

fn negamax(pos: &Position, depth: u32, min: i16, max: i16) -> i16 {
	let color = pos.side_to_move();
	if depth == 0 {
		return eval(pos.get_board(), color);
	}
	let moves = pos.gen_pseudolegal();
	if moves.len() == 0 {
		if pos.is_in_check(color) {
			return -std::i16::MAX; // checkmate
		} else {
			return 0; // stalemate
		}
	}
	let mut cur_max = min;
	for mov in moves {
		let mut pos2 = pos.clone();
		pos2.apply_move(&mov);
		let score = -negamax(&pos2, depth - 1, -max, -cur_max);
		if score > cur_max {
			cur_max = score;
			if cur_max >= max {
				return max;
			}
		}
	}
	return cur_max;
}

pub struct SimpleAi {
	depth: u32,
}
impl SimpleAi {
	pub fn new(depth: u32) -> SimpleAi {
		SimpleAi { depth }
	}
}
impl ChessAi for SimpleAi {
	fn name(&self) -> String {
		return format!("SimpleAI {}", self.depth);
	}
	fn pick_move(&mut self, pos: &Position, legal_moves: &[Move]) -> Move {
		let t0 = Instant::now();
		let mut max = std::i16::MIN;
		let mut best_move = None;
		for mov in legal_moves {
			let mut pos2 = pos.clone();
			pos2.apply_move(mov);
			let score = -negamax(&pos2, self.depth - 1, -std::i16::MAX, std::i16::MAX);
			if score > max {
				max = score;
				best_move = Some(mov);
			}
		}
		println!("SimpleAi ({}): search completed in {} ms",
			pos.side_to_move(),
			(Instant::now() - t0).as_millis());
		best_move.unwrap().clone()
	}
}