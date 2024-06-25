use std::{fmt::{Display, Write}, time::Duration};

use chesslib::{ai::{self, ChessAi}, game::Position};
use reqwest::{blocking::Client, Method, Url};
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

impl Bot {
	fn request<Res: DeserializeOwned>(&self, req: BotReq) -> Result<Res, String> {
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
			return res.json::<Res>().map_err(|e| format!("unexpected response: {}", e));
		}
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
	let client = Client::new();
	let bot = Bot { token, client };


	#[derive(Deserialize)]
	struct AccountData {
		username: String,
	}
	let data: AccountData = bot.request(get("account"))?;
	println!("Playing as {}", data.username);


	#[derive(Deserialize, Debug)]
	#[serde(rename_all = "camelCase")]
	struct PlayingData {
		now_playing: Vec<GameData>,
	}
	#[derive(Deserialize, Debug)]
	#[serde(rename_all = "camelCase")]
	struct GameData {
		game_id: String,
		is_my_turn: bool,
		fen: String,
	}
	let data: PlayingData = bot.request(get("account/playing").query("nb", 10))?;
	
	for game in data.now_playing {
		if game.is_my_turn {
			println!("Game {}: {}", game.game_id, game.fen);
			let pos = Position::from_fen(&game.fen)
				.ok_or_else(|| format!("failed to parse FEN"))?;
			println!("Thinking...");
			let moves = pos.gen_legal();
			let mov = ai::SimpleAi::new(6).pick_move(&pos, &moves);
			println!("Move: {}", mov);

			let data: serde_json::Value = bot.request(post("bot/game")
				.path(game.game_id).path("move").path(mov.uci_notation()))?;
			println!("Response: {}", data);

			break;
		}
	}

	/*
	#[derive(Deserialize, Debug)]
	struct ChallengeAiData {
		id: String,
		fen: String,
		player: String,
	}
	let data: ChallengeAiData = bot.request(post("challenge/ai").body("level", 1))?;
	println!("Response: {:?}", data);
	*/

	Ok(())
}

fn main() {
	if let Err(err) = run_bot() {
		eprintln!("{BRIGHT_RED}error:{RESET} {}", err);
		std::process::exit(1);
	}
}