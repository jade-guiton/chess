use std::{
	collections::HashMap,
	fmt::{Display, Write as _},
	fs::{File, OpenOptions},
	io::{Read, Write as _},
	sync::mpsc,
	thread::JoinHandle,
	time::{Duration, Instant}
};

use chesslib::{ai::ChessAi, game::Position, state::{Color, Move}};
use reqwest::{blocking::{Client, Response}, Method, Url};
use serde::{de::DeserializeOwned, Deserialize};
use toml::Table;

const BRIGHT_RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[1;33m";
const RESET: &str = "\x1b[0m";

fn config_get_integer(config: &Table, name: &str) -> Result<i64, String> {
	let val = config.get(name)
		.ok_or_else(|| format!("bot_config.toml: no {} key", name))?
		.clone();
	if let toml::Value::Integer(val) = val {
		Ok(val)
	 } else {
		Err(format!("bot_config.toml: {} is not an integer", name))
	}
}
struct Config {
	token: String,
	depth: u32,
	play_rated: bool,
	clock_initial: i64,
	clock_increment: i64,
	idle_timeout: u64,
	challenge_timeout: u64,
}
fn load_config() -> Result<Config, String> {
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

	let depth = config_get_integer(&config, "SEARCH_DEPTH")?;
	if depth < 1 {
		return Err(format!("bot_config.toml: SEARCH_DEPTH is not positive"));
	}
	let depth = depth as u32;

	let play_rated = config.get("PLAY_RATED")
		.ok_or_else(|| format!("bot_config.toml: no PLAY_RATED key"))?
		.clone();
	let play_rated = if let toml::Value::Boolean(play_rated) = play_rated { play_rated } else {
		return Err(format!("bot_config.toml: PLAY_RATED is not a boolean"));
	};

	let clock_initial = config_get_integer(&config, "CLOCK_INITIAL")?;
	if !([0,15,30,45,60,90].contains(&clock_initial) || clock_initial % 60 == 0) || clock_initial > 10800 {
		return Err(format!("bot_config.toml: CLOCK_INITIAL is not a valid value\n(0,15,30,45,90 or a multiple of 60 up to 10800)"));
	}
	let clock_increment = config_get_integer(&config, "CLOCK_INCREMENT")?;
	if clock_increment < 0 || clock_increment > 60 {
		return Err(format!("bot_config.toml: CLOCK_INCREMENT is not in [0, 60]"));
	}

	let idle_timeout = config_get_integer(&config, "IDLE_TIMEOUT")?;
	if idle_timeout < 0 {
		return Err(format!("bot_config.toml: IDLE_TIMEOUT is negative"));
	}
	let idle_timeout = idle_timeout as u64;

	let challenge_timeout = config_get_integer(&config, "CHALLENGE_TIMEOUT")?;
	if challenge_timeout < 0 {
		return Err(format!("bot_config.toml: CHALLENGE_TIMEOUT is negative"));
	}
	let challenge_timeout = challenge_timeout as u64;

	Ok(Config {
		token, depth, play_rated, clock_initial, clock_increment, idle_timeout, challenge_timeout,
	})
}

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

struct JsonStream<Res: DeserializeOwned + Send + 'static> {
	_listener: JoinHandle<()>,
	recv: mpsc::Receiver<Result<Res, String>>,
}
impl<Res: DeserializeOwned + Send + 'static> JsonStream<Res> {
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
					if read == 0 {
						return;
					}
					buf.extend_from_slice(&chunk[..read]);
				}
			}
		});
		JsonStream { _listener: listener, recv }
	}
	fn read(&self) -> Option<Result<Res, String>> {
		self.recv.recv().ok()
	}
	fn read_timeout(&self, dur: Duration) -> Option<Result<Res, String>> {
		self.recv.recv_timeout(dur).ok()
	}
}

struct BotClient {
	token: String,
	client: Client,
}
impl BotClient {
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

	fn stream_json<Res: DeserializeOwned + Send + 'static>(&self, req: BotReq) -> Result<JsonStream<Res>, String> {
		Ok(JsonStream::new(self.request(req)?))
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

#[derive(Deserialize)]
struct AccountData {
	id: String,
	username: String,
	perfs: HashMap<String, PerfData>,
}
#[derive(Deserialize)]
struct PerfData {
	rating: i32,
	rd: i32,
}

struct Bot {
	config: Config,
	client: BotClient,
	blacklist_file: File,
	blacklist: Vec<String>,
	account: AccountData
}

fn load_bot() -> Result<Bot, String> {
	let config = load_config()?;

	let mut blacklist_file = OpenOptions::new().read(true).append(true).create(true)
		.open("bot_blacklist.txt")
		.map_err(|err| format!("could not open bot_blacklist.txt: {}", err))?;
	let mut blacklist = String::new();
	blacklist_file.read_to_string(&mut blacklist).unwrap();
	let blacklist: Vec<String> = blacklist.lines().map(|s| s.to_owned()).collect();

	let client = BotClient {
		token: config.token.clone(),
		client: Client::new(),
	};

	let account: AccountData = client.json(get("account"))?;
	let blitz_perf = &account.perfs["blitz"];
	println!("playing as {} (blitz rating {} / dev {})",
		account.username, blitz_perf.rating, blitz_perf.rd);

	Ok(Bot {
		config,
		client, 
		blacklist_file,
		blacklist,
		account
	})
}

impl Bot {
	fn play_game(&self, game_id: &str) -> Result<(), String> {
		let ai = chesslib::ai::SimpleAi::new(self.config.depth);

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
			ChatLine {
				username: String,
				text: String,
			},
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

		let stream = self.client.stream_json(get("bot/game/stream").path(&game_id))?;

		let event: GameEvent = stream.read()
			.ok_or_else(|| format!("game event stream closed unexpectedly"))??;

		let (mut pos, mut history, color) = if let GameEvent::GameFull { initial_fen, state, white, black } = event {
			println!("initial: {}", initial_fen);
			println!("history: {}", state.moves);
			println!("white: {} / black: {}", white.id.as_deref().unwrap_or("?"), black.id.as_deref().unwrap_or("?"));
			println!("status: {}", state.status);

			let color = if white.id.as_ref() == Some(&self.account.id) {
				Color::White
			} else if black.id.as_ref() == Some(&self.account.id) {
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

			if pos.side_to_move() == color && !moves.is_empty() {
				println!("thinking...");
				let mov = ai.pick_move(&pos, &moves);
				println!("playing {}", mov);
				self.client.action(post("bot/game")
					.path(&game_id).path("move").path(mov.uci_notation()))?;
			}

			loop {
				let event: GameEvent = stream.read()
					.ok_or_else(|| format!("game event stream closed unexpectedly"))??;

				match event {
					GameEvent::GameState(state) => {
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
					},
					GameEvent::ChatLine { username, text } =>
						println!("chat: [{}] {}", username, text),
					_ =>
						println!("unexpected game event: {event:?}"),
				}
			}
		}

		Ok(())
	}

	fn find_active_game(&self) -> Result<Option<String>, String> {
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
		let playing: PlayingData = self.client.json(get("account/playing").query("nb", 10))?;

		Ok(playing.now_playing.first().map(|g| g.game_id.clone()))
	}

	fn find_bot_opponent(&self) -> Result<Option<String>, String> {
		let blitz_rating = self.account.perfs["blitz"].rating;
		let min_rating = blitz_rating - 100;
		let max_rating = blitz_rating + 100;
		println!("searching for bot with rating in [{}, {}]...", min_rating, max_rating);

		let stream = self.client.stream_json::<AccountData>(get("bot/online"))?;		
		let mut matching_bots = vec![];
		while let Some(res) = stream.read() {
			let bot = res?;
			let blitz_rating = bot.perfs["blitz"].rating;
			if blitz_rating >= min_rating && blitz_rating <= max_rating
				&& self.blacklist.iter().all(|un| un != &bot.username) {
				matching_bots.push(bot.username);
				print!("o");
			} else {
				print!("x");
			}
			std::io::stdout().flush().unwrap();
		}
		println!("");
		Ok(if matching_bots.is_empty() {
			None
		} else {
			let name = matching_bots[rand::random::<usize>() % matching_bots.len()].clone();
			Some(name)
		})
	}

	fn challenge_user(&mut self, username: &str) -> Result<Option<String>, String> {
		println!("challenging user {}", username);

		#[derive(Deserialize, Debug)]
		#[serde(untagged)]
		enum ChallengeStreamData {
			Challenge {
				id: String,
			},
			#[serde(rename_all = "camelCase")]
			Response {
				done: String,
			},
		}
		let stream: JsonStream<ChallengeStreamData> = self.client.stream_json(post("challenge")
			.path(username)
			.body("rated", self.config.play_rated)
			.body("clock.limit", self.config.clock_initial)
			.body("clock.increment", self.config.clock_increment)
			.body("color", "random")
			.body("keepAliveStream", true)
		)?;
		let msg = stream.read_timeout(Duration::from_secs(5))
			.ok_or_else(|| format!("creation of challenge timed out"))??;
		let game_id;
		if let ChallengeStreamData::Challenge { id } = msg {
			game_id = id
		} else {
			return Err(format!("unexpected message in challenge event stream"));
		}
		println!("challenge sent, waiting...");

		let status;
		if let Some(msg) = stream.read_timeout(Duration::from_secs(self.config.challenge_timeout)) {
			if let ChallengeStreamData::Response { done } = msg? {
				status = done;
			} else {
				return Err(format!("unexpected message in challenge event stream"));
			}
		} else {
			println!("challenge timed out.");
			return Ok(None);
		}
		if status != "accepted" {
			println!("challenge was not accepted (status: {})", status);
			println!("adding bot {} to blacklist", username);
			write!(self.blacklist_file, "{}\n", username)
				.map_err(|err| format!("could not write to blacklist file: {}", err))?;
			self.blacklist.push(username.to_owned());
			return Ok(None);
		}

		Ok(Some(game_id))
	}
}

#[derive(Deserialize, Debug)]
struct Challenge {
	id: String,
	status: String,
	speed: String,
	variant: Variant,
	challenger: ChallengeUser,
}
#[derive(Deserialize, Debug)]
struct ChallengeUser {
	name: String,
}
#[derive(Deserialize, Debug)]
struct Variant {
	key: String,
}

impl Bot {
	fn process_challenge(&self, chal: &Challenge) -> Result<bool, String> {
		if chal.status == "created" || chal.status == "offline" {
			if chal.speed != "blitz" {
				println!("declining challenge {} from {}: not blitz", chal.id, chal.challenger.name);
				self.client.action(post("challenge")
					.path(&chal.id).path("decline")
					.body("reason", "declineTimeControl")
				)?;
			} else if chal.variant.key != "standard" {
				println!("declining challenge {} from {}: not standard", chal.id, chal.challenger.name);
				self.client.action(post("challenge")
					.path(&chal.id).path("decline")
					.body("reason", "declineStandard")
				)?;
			} else if chal.status == "created" {
				println!("accepting challenge {} from {}", chal.id, chal.challenger.name);
				self.client.action(post("challenge")
					.path(&chal.id).path("accept")
				)?;
				return Ok(true);
			}
		}
		Ok(false)
	}

	fn await_challenge(&self) -> Result<bool, String> {
		#[derive(Deserialize, Debug)]
		struct Challenges {
			r#in: Vec<Challenge>,
		}
		let challenges: Challenges = self.client.json(get("challenge"))?;
		for chal in challenges.r#in {
			if self.process_challenge(&chal)? {
				return Ok(true);
			}
		}

		#[derive(Deserialize, Debug)]
		#[serde(tag = "type", rename_all = "camelCase")]
		enum GameEvent {
			GameStart,
			GameFinish,
			Challenge {
				challenge: Challenge,
			},
			ChallengeCanceled,
			ChallengeDeclined,
		}

		let timeout_instant = Instant::now() + Duration::from_secs(self.config.idle_timeout);
		let stream = self.client.stream_json(get("stream/event"))?;
		while let Some(res) = stream.read_timeout(timeout_instant - Instant::now()) {
			let event: GameEvent = res?;
			if let GameEvent::Challenge { challenge } = event {
				if challenge.challenger.name != self.account.username && self.process_challenge(&challenge)? {
					return Ok(true);
				}
			} else if let GameEvent::GameStart = event {
				return Ok(true);
			} else {
				println!("event: {:?}", event);
			}
		}
		Ok(false)
	}
}

fn main() {
	if let Err(err) = || -> Result<(), String> {
		let mut bot = load_bot()?;
		loop {
			if let Some(game_id) = bot.find_active_game()? {
				println!("active game: {}", game_id);
				if let Err(err) = bot.play_game(&game_id) {
					eprintln!("{BRIGHT_RED}error:{RESET} {}", err);
				}
			} else {
				println!("no active game, waiting for challenges...");
				if bot.await_challenge()? { continue }
				println!("received no challenges, starting matchmaking");
				if let Some(username) = bot.find_bot_opponent()? {
					bot.challenge_user(&username)?;
				} else {
					println!("foud no suitable opponents.");
				}
			}
		}
	}() {
		eprintln!("{BRIGHT_RED}error:{RESET} {}", err);
		std::process::exit(1);
	}
}