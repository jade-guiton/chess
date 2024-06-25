use std::fmt;

use chesslib::ai::{ParallelAi, SimpleAi};
use chesslib::state::{Move, PieceType, Square};
use chesslib::game::Position;
use sdl2::{
	event::Event,
	gfx::primitives::DrawRenderer,
	image::LoadTexture,
	mouse::MouseButton,
	pixels::Color,
	rect::Rect,
	rwops::RWops
};

const SPRITE_SIZE: u32 = 16;
const SPRITE_ZOOM: u32 = 5;
const TILE_SIZE: u32 = SPRITE_SIZE * SPRITE_ZOOM;
const STATUS_BAR_HEIGHT: u32 = 12 * SPRITE_ZOOM;
const STATUS_FONT_SIZE: u16 = 4 * SPRITE_ZOOM as u16;
const WINDOW_WIDTH: u32 = TILE_SIZE*8;
const WINDOW_HEIGHT: u32 = TILE_SIZE*8 + STATUS_BAR_HEIGHT;

const BOT_DELAY: i64 = 30;

fn hsv_to_rgb(h: f32, s: f32, v: f32, a: f32) -> Color {
	assert!(0.0 <= s && s <= 1.0 && 0.0 <= v && v <= 1.0);
	let h2 = (h % 1.0) * 6.0;
	let c = v * s;
	let x = c * (1.0 - (h2 % 2.0 - 1.0).abs());
	let m = v - c;
	let (r1,g1,b1) = if h2 < 1.0 {
		(c,x,0.0)
	} else if h2 < 2.0 {
		(x,c,0.0)
	} else if h2 < 3.0 {
		(0.0,c,x)
	} else if h2 < 4.0 {
		(0.0,x,c)
	} else if h2 < 5.0 {
		(x,0.0,c)
	} else {
		(c,0.0,x)
	};
	Color::RGBA(
		((r1 + m) * 255.0).round() as u8,
		((g1 + m) * 255.0).round() as u8,
		((b1 + m) * 255.0).round() as u8,
		(a * 255.0).round() as u8,
	)
}

enum PlayerType {
	User,
	Bot(ParallelAi),
}
impl PlayerType {
	fn status(&self) -> String {
		match self {
			PlayerType::User => "Drag and drop a piece to make a move".to_string(),
			PlayerType::Bot(_) => "Thinking...".to_string(),
		}
	}
}
impl fmt::Display for PlayerType {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			PlayerType::User => write!(f, "User"),
			PlayerType::Bot(bot) => write!(f, "{}", bot.name()),
		}
	}
}

#[derive(Clone)]
struct Promotion {
	move_to: Square,
	choices: Vec<PieceType>,
}

struct App<'a> {
	canvas: sdl2::render::Canvas<sdl2::video::Window>,
	events: sdl2::EventPump,
	texture_creator: &'a sdl2::render::TextureCreator<sdl2::video::WindowContext>,
	atlas_texture: sdl2::render::Texture<'a>,
	font: sdl2::ttf::Font<'a,'static>,

	position: Position,
	players: [PlayerType; 2],
	timer: i64,
	move_from: Option<Square>,
	promotion: Option<Promotion>,
	prev_move: Option<Move>,
}

impl<'a> App<'a> {
	fn new(
		canvas: sdl2::render::Canvas<sdl2::video::Window>,
		events: sdl2::EventPump,
		texture_creator: &'a sdl2::render::TextureCreator<sdl2::video::WindowContext>,
		atlas_texture: sdl2::render::Texture<'a>,
		font: sdl2::ttf::Font<'a, 'static>,
	) -> Self {
		App {
			canvas, events, texture_creator, atlas_texture, font,
			position: Position::from_fen("nnnnnnnn/PPPPPPPP/8/8/8/8/8/K6k w - - 0 1").unwrap(),
			players: [
				PlayerType::User,
				PlayerType::Bot(ParallelAi::new(SimpleAi::new(6))),
			],
			timer: 0,
			move_from: None,
			promotion: None,
			prev_move: None,
		}
	}
}

impl App<'_> {
	fn draw_sprite(&mut self, sx: u8, sy: u8, x: u8, y: u8) {
		self.canvas.copy(&self.atlas_texture,
			Rect::new((sx as u32 * SPRITE_SIZE) as i32, (sy as u32 * SPRITE_SIZE) as i32, SPRITE_SIZE, SPRITE_SIZE),
			Rect::new((x as u32 * TILE_SIZE) as i32, ((7 - y as u32)*TILE_SIZE) as i32, TILE_SIZE, TILE_SIZE)).unwrap();
	}

	fn draw_move(&mut self, from: Square, to: Square, color: Color) {
		let x1 = from.file() as u32 * TILE_SIZE + TILE_SIZE/2;
		let y1 = (7 - from.rank()) as u32 * TILE_SIZE + TILE_SIZE/2;
		let x2 = to.file() as u32 * TILE_SIZE + TILE_SIZE/2;
		let y2 = (7 - to.rank()) as u32 * TILE_SIZE + TILE_SIZE/2;
		
		self.canvas.thick_line(x1 as i16, y1 as i16, x2 as i16, y2 as i16,
			(TILE_SIZE/10) as u8, color).unwrap();
	}

	fn draw_text(&mut self, text: &str, x: i32, y: i32) {
		let text_surf = self.font.render(text).blended(Color::WHITE).unwrap();
		let text_tex = self.texture_creator.create_texture_from_surface(&text_surf).unwrap();
		self.canvas.copy(&text_tex, None, Rect::new(
			x, y - text_surf.height() as i32 / 2,
			text_surf.width(), text_surf.height(),
		)).unwrap();
	}

	fn make_move(&mut self, mov: Move) {
		self.position.apply_move(&mov);
		self.prev_move = Some(mov);
		self.timer = 0;
	}

	fn process_frame(&mut self) -> bool {
		self.canvas.set_draw_color(Color::BLACK);
		self.canvas.clear();

		let pieces = self.position.get_board().get_pieces();
		for x in 0..8u8 {
			for y in 0..8u8 {
				self.draw_sprite(3, (x+y) % 2, x, y); // board tile
				if let Some(piece) = pieces[Square::at(x as u8, y as u8)] {
					let type_idx = piece.ptype as u8;
					let color_idx = piece.color as u8;
					self.draw_sprite(type_idx % 3, type_idx / 3 + 2 * color_idx, x, y);
				}
			}
		}

		if let Some(mov) = self.prev_move {
			self.draw_move(mov.from, mov.to, hsv_to_rgb(mov.ptype as u8 as f32 / 6.0, 1.0, 1.0, 0.5));
		}

		let moves = self.position.gen_legal();
		let player = self.position.side_to_move();
		let user_to_move = matches!(self.players[player], PlayerType::User);

		if user_to_move {
			if let Some(from) = self.move_from {
				self.draw_sprite(3, 2, from.file(), from.rank());
				if let Some(promotion) = self.promotion.clone() { // choosing promotion
					self.draw_sprite(3, 3, promotion.move_to.file(), promotion.move_to.rank());
					self.draw_move(from, promotion.move_to, Color::RGBA(255, 255, 255, 128));

					self.canvas.set_draw_color(Color::RGBA(0, 0, 0, 64));
					self.canvas.fill_rect(None).unwrap();

					for (i, ptype) in promotion.choices.into_iter().enumerate() {
						let spr_idx = ptype as u8;
						self.draw_sprite(spr_idx % 3, player as u8 * 2 + spr_idx / 3, 2 + i as u8, 4);
					}
				} else {
					for mov in &moves {
						if mov.from == from {
							self.draw_sprite(3, 3, mov.to.file(), mov.to.rank());
						}
					}
				}
			} else {
				for mov in &moves {
					self.draw_sprite(3, 3, mov.from.file(), mov.from.rank());
				}
			}
		}

		let line1 = format!("Ply {:<3} | {} ({})'s turn",
			self.position.get_ply(),
			player, self.players[player]
		);
		let line2 = if moves.len() == 0 {
			if self.position.is_in_check(player) {
				format!("Checkmate! Win for {}.", player.opponent())
			} else {
				format!("It's a draw.")
			}
		} else {
			self.players[player].status()
		};
		let status_x = STATUS_FONT_SIZE as i32 / 2;
		let status_y = 8 * TILE_SIZE as i32 + STATUS_BAR_HEIGHT as i32 / 2;
		self.draw_text(&line1, status_x, status_y - STATUS_FONT_SIZE as i32 * 2 / 3);
		self.draw_text(&line2, status_x, status_y + STATUS_FONT_SIZE as i32 * 2 / 3);

		self.canvas.present();

		loop {
			let event = if let Some(event) = self.events.poll_event() { event } else { break };
			match event {
				Event::Quit { .. } => return false,
				Event::MouseButtonDown { mouse_btn, x, y, .. } => {
					if mouse_btn == MouseButton::Left
						&& x >= 0 && y >= 0 && x < 8*TILE_SIZE as i32 && y < 8*TILE_SIZE as i32
						&& user_to_move {
						let gx = x as u32 / TILE_SIZE;
						let gy = y as u32 / TILE_SIZE;
						if let Some(promotion) = &self.promotion {
							if gy == 3 && gx >= 2 && gx < 2 + promotion.choices.len() as u32 {
								let ptype = promotion.choices[gx as usize - 2];
								
								let matching: Vec<Move> = moves.iter().filter(|m|
									m.from == self.move_from.unwrap()
									&& m.to == promotion.move_to
									&& m.special.get_promotion() == Some(ptype)
								).copied().collect();
								debug_assert!(matching.len() == 1);

								self.move_from = None;
								self.promotion = None;
								
								self.make_move(matching[0]);
							}
						} else if self.move_from.is_none() {
							let squ = Square::at(gx as u8, 7 - gy as u8);
							if moves.iter().any(|m| m.from == squ) {
								self.move_from = Some(squ);
							}
						}
					}
				},
				Event::MouseButtonUp { mouse_btn, x, y, .. } => {
					if mouse_btn == MouseButton::Left && user_to_move && self.promotion.is_none() {
						if let Some(from) = self.move_from {
							if x >= 0 && y >= 0 && x < 8*TILE_SIZE as i32 && y < 8*TILE_SIZE as i32 {
								let gx = x as u32 / TILE_SIZE;
								let gy = y as u32 / TILE_SIZE;
								let squ = Square::at(gx as u8, 7 - gy as u8);
								let mut matching_moves = Vec::with_capacity(1);
								for mov in moves.iter() {
									if mov.from == from && mov.to == squ {
										matching_moves.push(*mov);
									}
								}
								if matching_moves.is_empty() {
									self.move_from = None;
								} else if matching_moves.len() == 1 {
									self.move_from = None;
									if let Some(mov) = matching_moves.first() {
										self.make_move(*mov);
									}
								} else {
									let ptypes: Vec<PieceType> = matching_moves.into_iter().map(|m| m.special.get_promotion()
										.expect("non-promotion move found among multiple matching moves")).collect();
									assert!(ptypes.len() == 4, "!= 4 promotions found");
									self.promotion = Some(Promotion {
										move_to: squ,
										choices: ptypes,
									})
								}
							} else {
								self.move_from = None;
							}
						}
					}
				},
				_ => {},
			}
		}

		if let PlayerType::Bot(bot) = &mut self.players[player] {
			if bot.is_thinking() {
				if self.timer >= BOT_DELAY {
					if let Some(mov) = bot.try_get_result() {
						self.make_move(mov);
					}
				}
			} else if !moves.is_empty() {
				bot.pick_move_async(&self.position, &moves);
			}
		}

		self.timer += 1;

		return true;
	}
}

fn main() {
	let sdl = sdl2::init().unwrap();
	let video = sdl.video().unwrap();
	let window = video.window("Pyxyne's Chess Engine", WINDOW_WIDTH, WINDOW_HEIGHT)
		.position_centered()
		.build().unwrap();
	let canvas = window.into_canvas()
		.present_vsync()
		.build().unwrap();
	let texture_creator = canvas.texture_creator();
	let atlas_texture = {
		let _ = sdl2::image::init(sdl2::image::InitFlag::PNG).unwrap();
		texture_creator.load_texture_bytes(include_bytes!("../res/sprites.png")).unwrap()
	};
	let ttf = sdl2::ttf::init().unwrap();
	let font = {
		let rwops = RWops::from_bytes(include_bytes!("../res/RobotoMono.ttf")).unwrap();
		ttf.load_font_from_rwops(rwops, STATUS_FONT_SIZE).unwrap()
	};
	let events = sdl.event_pump().unwrap();

	let mut app = App::new(canvas, events, &texture_creator, atlas_texture, font);
	while app.process_frame() {}
}
