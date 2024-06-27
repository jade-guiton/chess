use chesslib::{ai::{ChessAi, SimpleAi}, game::Position};

fn main() {
	let mut pos = Position::from_fen(Position::FEN_INITIAL).unwrap();
	let ai = SimpleAi::new(5);
	loop {
		let moves = pos.gen_legal();
		if moves.len() == 0 {
			break;
		}
		let mov = ai.pick_move(&pos, &moves);
		pos.apply_move(&mov);
	}
}