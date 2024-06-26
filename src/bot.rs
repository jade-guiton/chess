use std::{fmt::{Display, Write}, io::Read, sync::mpsc, thread::JoinHandle, time::Duration};

use chesslib::{ai::ChessAi, game::Position, state::{Color, Move}};
use reqwest::{blocking::{Client, Response}, Method, Url};
use serde::{de::DeserializeOwned, Deserialize};
use toml::Table;

const BRIGHT_RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[1;33m";
const RESET: &str = "\x1b[0m";

struct BotReq {
	method: Method,
	url: Url,
	body: Option<Vec<(String, String)>>,
}
impl BotReq {
	fn new(method: Method, url: &str) -> Self {
		BotReq {
			method,
			url: Url::parse(&format!("https://lichess.org/api/{}", url)).expect("invalid base URL"),
			body: None,
		}
	}
	fn path(mut self, part: impl Display) -> Self {
		self.url.path_segments_mut().unwrap().push(&format!("{}", part));
		self
	}
	fn query(mut self, key: &'static str, value: impl Display) -> Self {
		self.url.query_pairs_mut().append_pair(key, &format!("{}", value));
		self
	}
	fn body(mut self, key: &'static str, value: impl Display) -> Self {
		if self.body.is_none() {
			self.body = Some(vec![]);
		}
		self.body.as_mut().unwrap().push((key.to_owned(), format!("{}", value)));
		self
	}
}
fn get(url: &str) -> BotReq {
	BotReq::new(Method::GET, url)
}
fn post(url: &str) -> BotReq {
	BotReq::new(Method::POST, url)
}

struct Bot {
	token: String,
	client: Client,
}

struct Stream<Res: DeserializeOwned + Send + 'static> {
	_listener: JoinHandle<()>,
	recv: mpsc::Receiver<Result<Res, String>>,
}
impl<Res: DeserializeOwned + Send + 'static> Stream<Res> {
	fn new(mut res: Response) -> Self {
		let (send, recv) = mpsc::channel::<Result<Res, String>>();
		let listener = std::thread::spawn(move || {
			let mut buf = vec![];
			loop {
				if let Some(i) = buf.iter().position(|b| *b == b'\n') {
					if i > 0 {
						let msg = &buf[..i];
						let msg = serde_json::from_slice(msg)
							.map_err(|e| format!("failed to deserialize ndjson: {}\n{}", e, String::from_utf8_lossy(msg)));
						if let Err(_) = send.send(msg) {
							return;
						}
					}
					buf.drain(0..(i+1));
				} else {
					let mut chunk = [0u8; 256];
					let read = res.read(&mut chunk)
						.map_err(|e| format!("failed to read from response: {}", e))
						.unwrap();
					buf.extend_from_slice(&chunk[..read]);
				}
			}
		});
		Stream { _listener: listener, recv }
	}
	fn read(&self) -> Result<Res, String> {
		self.recv.recv().expect("listener thread is dead")
	}
}

impl Bot {
	fn request(&self, req: BotReq) -> Result<Response, String> {
		loop {
			let mut b = self.client.request(req.method.clone(), req.url.clone())
				.bearer_auth(&self.token);
			if let Some(body) = &req.body {
				b = b.form(body);
			}
			let res = b.send().map_err(|e| format!("failed to send request: {}", e))?;
			let status = res.status();
			if status.as_u16() == 429 {
				eprintln!("{YELLOW}warning:{RESET} received Too Many Requests, waiting 1 minute");
				std::thread::sleep(Duration::from_secs(60));
				continue
			} else if !status.is_success() {
				let mut msg = format!("HTTP {}", status.as_u16());
				if let Some(reason) = status.canonical_reason() {
					write!(msg, " {}", reason).unwrap();
				}
				#[derive(Deserialize)]
				struct ErrorData {
					error: String,
				}
				if let Ok(data) = res.json::<ErrorData>() {
					write!(msg, ": {}", data.error).unwrap();
				}
				return Err(msg);
			}
			return Ok(res);
		}
	}

	fn json<Res: DeserializeOwned>(&self, req: BotReq) -> Result<Res, String> {
		let res = self.request(req)?;
		res.json::<Res>().map_err(|e| format!("unexpected response: {}", e))
	}

	fn stream_json<Res: DeserializeOwned + Send + 'static>(&self, req: BotReq) -> Result<Stream<Res>, String> {
		Ok(Stream::new(self.request(req)?))
	}

	fn action(&self, req: BotReq) -> Result<(), String> {
		#[derive(Deserialize, Debug)]
		struct OkRes { ok: bool }
		let data: OkRes = self.json(req)?;
		if !data.ok {
			return Err(format!("unexpected ok=false in 200 response"));
		}
		Ok(())
	}
}

fn run_bot() -> Result<(), String> {
	let config = std::fs::read_to_string("bot_config.toml")
		.map_err(|e| format!("could not read bot_config.toml: {}", e))?
		.parse::<Table>()
		.map_err(|e| format!("bot_config.toml: invalid syntax: {}", e))?;

	let token = config.get("BOT_TOKEN")
		.ok_or_else(|| format!("bot_config.toml: no BOT_TOKEN key"))?
		.clone();
	let token = if let toml::Value::String(token) = token { token } else {
		return Err(format!("bot_config.toml: BOT_TOKEN is not a string"));
	};
	let depth = config.get("SEARCH_DEPTH")
		.ok_or_else(|| format!("bot_config.toml: no SEARCH_DEPTH key"))?
		.clone();
	let depth = if let toml::Value::Integer(depth) = depth { depth } else {
		return Err(format!("bot_config.toml: SEARCH_DEPTH is not an integer"));
	};
	if depth < 1 {
		return Err(format!("bot_config.toml: SEARCH_DEPTH is not positive"));
	}
	let depth = depth as u32;

	let ai = chesslib::ai::SimpleAi::new(depth);

	let client = Client::new();
	let bot = Bot { token, client };


	#[derive(Deserialize)]
	struct AccountData {
		id: String,
		username: String,
	}
	let account: AccountData = bot.json(get("account"))?;
	println!("playing as {}", account.username);

	#[derive(Deserialize, Debug)]
	#[serde(rename_all = "camelCase")]
	struct PlayingData {
		now_playing: Vec<GameData>,
	}
	#[derive(Deserialize, Debug)]
	#[serde(rename_all = "camelCase")]
	struct GameData {
		game_id: String,
	}
	let playing: PlayingData = bot.json(get("account/playing").query("nb", 10))?;

	let game_id = if !playing.now_playing.is_empty() {
		let id = playing.now_playing[0].game_id.clone();
		println!("game: {}", id);
		id
	} else {
		println!("no active game, challenging AI");

		#[derive(Deserialize, Debug)]
		struct ChallengeAiData {
			id: String,
		}
		let data: ChallengeAiData = bot.json(post("challenge/ai").body("level", 1))?;
		println!("New game: {}", data.id);
		data.id
	};

	#[derive(Deserialize, Debug)]
	#[serde(tag = "type", rename_all = "camelCase")]
	enum GameEvent {
		#[serde(rename_all = "camelCase")]
		GameFull {
			initial_fen: String,
			state: GameState,
			white: PlayerData,
			black: PlayerData,
		},
		GameState(GameState),
		ChatLine,
		OpponentGone,
	}
	#[derive(Deserialize, Debug)]
	#[serde(rename_all = "camelCase")]
	struct GameState {
		moves: String,
		status: String,
	}
	#[derive(Deserialize, Debug)]
	struct PlayerData {
		id: Option<String>,
	}

	let stream = bot.stream_json(get("bot/game/stream").path(&game_id))?;

	let event: GameEvent = stream.read()?;

	let (mut pos, mut history, color) = if let GameEvent::GameFull { initial_fen, state, white, black } = event {
		println!("initial: {}", initial_fen);
		println!("history: {}", state.moves);
		println!("white: {} / black: {}", white.id.as_deref().unwrap_or("?"), black.id.as_deref().unwrap_or("?"));
		println!("status: {}", state.status);

		let color = if white.id.as_ref() == Some(&account.id) {
			Color::White
		} else if black.id.as_ref() == Some(&account.id) {
			Color::Black
		} else {
			return Err(format!("bot is not a player in this game"));
		};
		
		let mut pos = Position::from_fen(
			if initial_fen == "startpos" { Position::FEN_INITIAL } else { &initial_fen }
		).ok_or_else(|| format!("failed to parse initial FEN"))?;
		if state.status != "started" {
			return Err(format!("unexpected game status"));
		}

		let mut history = vec![];
		for mov_desc in state.moves.split_ascii_whitespace() {
			let moves = pos.gen_legal();
			let mov = Move::parse_uci(mov_desc, &moves)
				.map_err(|err| format!("failed to parse game history: {} is {}", mov_desc, err))?;
			history.push(mov_desc.to_owned());
			pos.apply_move(&mov);
		}

		(pos, history, color)
	} else {
		return Err(format!("unexpected first message: {event:?}"));
	};

	let mut moves = pos.gen_legal();
	'game_loop: loop {
		println!("state: {}", pos.to_fen());

		if pos.side_to_move() == color {
			println!("thinking...");
			let mov = ai.pick_move(&pos, &moves);
			println!("playing {}", mov);
			bot.action(post("bot/game")
				.path(&game_id).path("move").path(mov.uci_notation()))?;
		}

		loop {
			let event: GameEvent = stream.read()?;

			if let GameEvent::GameState(state) = event {
				if state.status != "started" {
					println!("game status: {}", state.status);
					break 'game_loop;
				}

				for (i, mov_desc) in state.moves.split_ascii_whitespace().enumerate() {
					if i < history.len() {
						if history[i] != mov_desc {
							return Err(format!("new game history does not match old one: {} / {}",
								history.join(" "), state.moves,
							));
						}
					} else {
						println!("move: {}", mov_desc);
						let mov = Move::parse_uci(mov_desc, &moves)
							.map_err(|err| format!("failed to parse new move: {}", err))?;
						history.push(mov_desc.to_owned());
						pos.apply_move(&mov);
						moves = pos.gen_legal();
					}
				}
				break;
			} else {
				println!("<- {event:?}");
			}
		}
	}

	Ok(())
}

fn main() {
	if let Err(err) = run_bot() {
		eprintln!("{BRIGHT_RED}error:{RESET} {}", err);
		std::process::exit(1);
	}
}