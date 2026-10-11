use std::collections::BTreeMap;

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, errors::ErrorKind, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use worker::*;
use world::Map;

/// Where the client and the editor load the bar's map from (`asset_server.load("maps/bar.ron")`)
/// and where the editor saves it. wrangler.toml sends this path here before looking for a file.
const MAP_PATH: &str = "/assets/maps/bar.ron";
/// The map in R2, once the editor has saved one.
const MAP_KEY: &str = "maps/bar.ron";
/// A map is a few KB; nothing near this is one.
const MAX_MAP_SIZE: usize = 1 << 20;

/// Files in `web/` are served before this runs (`[assets]` in wrangler.toml), so only
/// requests without a matching file land here, plus the map.
#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        // One Room Durable Object per bar room; the room name picks the instance.
        .get_async("/ws/:room", |req, ctx| async move {
            let Some(room) = ctx.param("room") else {
                return Response::error("Missing room", 400);
            };
            let stub = ctx.durable_object("ROOM")?.id_from_name(room)?.get_stub()?;
            stub.fetch_with_request(req).await
        })
        // WebRTC servers for the two players at a cabinet (web/room.js).
        .get_async(
            "/ice",
            |_req, ctx| async move { ice_servers(&ctx.env).await },
        )
        // FBNeo cores (/fbneo/<core>/fbneo.wasm), the cores built on their own (/supermodel/supermodel.wasm
        // for the Sega Model 3, /daytona/daytona.wasm for Daytona USA, /srally/srally.wasm for Sega Rally
        // Championship, /mame/mame.wasm for MAME, /flycast/flycast.wasm for the Sega NAOMI) and ROM sets
        // (/roms/mk2.zip, or a file in a folder of its own, /roms/vtennisg/gds-0011.chd) live in R2, each
        // with the content type it was uploaded with (Makefile).
        .get_async("/fbneo/*file", |req, ctx| serve_from_r2(req, ctx, "fbneo"))
        .get_async("/supermodel/*file", |req, ctx| {
            serve_from_r2(req, ctx, "supermodel")
        })
        .get_async("/mame/*file", |req, ctx| serve_from_r2(req, ctx, "mame"))
        .get_async("/daytona/*file", |req, ctx| {
            serve_from_r2(req, ctx, "daytona")
        })
        .get_async("/flycast/*file", |req, ctx| {
            serve_from_r2(req, ctx, "flycast")
        })
        .get_async("/srally/*file", |req, ctx| {
            serve_from_r2(req, ctx, "srally")
        })
        .get_async("/roms/*file", |req, ctx| serve_from_r2(req, ctx, "roms"))
        // The bar's map: the one last saved from the editor, or the one built with the site.
        .get_async(MAP_PATH, serve_map)
        .put_async(MAP_PATH, save_map)
        .run(req, env)
        .await
}

/// The map from R2, or the site's own copy until the editor has saved one. Browsers keep it
/// but ask each time whether it changed, since a save changes it without a deploy. `make dev`
/// reads the site's bucket for the games but sets MAP_FROM_R2 to false, so the map being
/// edited in assets/ shows, not the one saved on the site.
async fn serve_map(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let map_from_r2 = ctx.env.var("MAP_FROM_R2").map(|var| var.to_string());
    let saved = match map_from_r2.as_deref() {
        Ok("false") => None,
        _ => from_r2(&req, &ctx, MAP_KEY).await?,
    };
    let response = match saved {
        Some(response) => response,
        None => ctx.env.assets("ASSETS")?.fetch_request(req).await?,
    };
    // A fetched response's headers can't be changed in place; `Headers::clone` copies.
    let headers = response.headers().clone();
    headers.set("cache-control", "no-cache")?;
    Ok(response.with_headers(headers))
}

/// Stores the map the editor sends, once it's a map and the request came through Cloudflare
/// Access (`access_email`).
async fn save_map(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let email = match access_email(&req, &ctx.env).await? {
        Ok(email) => email,
        Err(refused) => return Ok(refused),
    };
    let text = req.text().await?;
    if text.len() > MAX_MAP_SIZE {
        return Response::error("Too big for a map", 413);
    }
    let map = match Map::from_ron(&text) {
        Ok(map) => map,
        Err(error) => return Response::error(format!("Not a map: {error}"), 400),
    };
    ctx.bucket("BUCKET")?
        .put(MAP_KEY, map.to_ron())
        .http_metadata(HttpMetadata {
            content_type: Some("text/plain; charset=utf-8".into()),
            ..Default::default()
        })
        .execute()
        .await?;
    console_log!("{email} saved the map");
    Response::empty()
}

/// What an Access token says about its user.
#[derive(Deserialize)]
struct Claims {
    /// Missing on a service token.
    email: Option<String>,
}

/// Who sent the request, from the Cloudflare Access token on it. The editor Worker sits behind
/// Access (README, Editor on the web), which signs a token onto every request it lets through;
/// the ACCESS_TEAM and ACCESS_AUD vars say whose tokens to trust, so a Worker without them
/// (the site itself) refuses to save.
async fn access_email(req: &Request, env: &Env) -> Result<std::result::Result<String, Response>> {
    // `wrangler dev` (make editor-dev) has no Access in front of it. A deployed Worker only
    // gets requests addressed to its own hostnames.
    if req.url()?.host_str() == Some("localhost") {
        return Ok(Ok("localhost".into()));
    }
    let var = |name: &str| {
        env.var(name)
            .map(|var| var.to_string())
            .ok()
            .filter(|value| !value.is_empty())
    };
    let (Some(team), Some(aud)) = (var("ACCESS_TEAM"), var("ACCESS_AUD")) else {
        let why = "Saving needs Cloudflare Access on this Worker: ACCESS_TEAM and ACCESS_AUD in wrangler.toml";
        return Ok(Err(Response::error(why, 503)?));
    };
    let Some(token) = req.headers().get("cf-access-jwt-assertion")? else {
        return Ok(Err(Response::error(
            "Not signed in to Cloudflare Access",
            401,
        )?));
    };
    // https://developers.cloudflare.com/cloudflare-one/identity/authorization-cookie/validating-json/
    let issuer = format!("https://{team}.cloudflareaccess.com");
    let certs = Url::parse(&format!("{issuer}/cdn-cgi/access/certs"))?;
    let keys: JwkSet = Fetch::Url(certs).send().await?.json().await?;
    match verify_access_token(&token, &keys, &issuer, &aud) {
        Ok(claims) => Ok(Ok(claims.email.unwrap_or_else(|| "A service token".into()))),
        Err(error) => {
            console_error!("Access token refused: {error}");
            Ok(Err(Response::error(
                "Cloudflare Access token refused",
                403,
            )?))
        }
    }
}

fn verify_access_token(
    token: &str,
    keys: &JwkSet,
    issuer: &str,
    aud: &str,
) -> jsonwebtoken::errors::Result<Claims> {
    let header = decode_header(token)?;
    let key = header
        .kid
        .as_deref()
        .and_then(|kid| keys.find(kid))
        .ok_or(ErrorKind::InvalidToken)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[aud]);
    Ok(decode::<Claims>(token, &DecodingKey::from_jwk(key)?, &validation)?.claims)
}

/// STUN, plus Cloudflare TURN once a TURN key is set up (the TURN_KEY_ID and TURN_KEY_API_TOKEN
/// secrets). TURN relays through the nearest Cloudflare location when two browsers can't reach
/// each other directly (strict NATs); without it those games go through the room instead,
/// wherever its Durable Object runs.
async fn ice_servers(env: &Env) -> Result<Response> {
    const STUN: &str = r#"{"iceServers":[{"urls":"stun:stun.cloudflare.com:3478"}]}"#;
    let json = |body: String| -> Result<Response> {
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("cache-control", "no-store")?;
        Ok(Response::ok(body)?.with_headers(headers))
    };
    let (Ok(key), Ok(token)) = (env.secret("TURN_KEY_ID"), env.secret("TURN_KEY_API_TOKEN")) else {
        return json(STUN.into());
    };
    // https://developers.cloudflare.com/realtime/turn/generate-credentials/
    let url = format!(
        "https://rtc.live.cloudflare.com/v1/turn/keys/{key}/credentials/generate-ice-servers"
    );
    let headers = Headers::new();
    headers.set("authorization", &format!("Bearer {token}"))?;
    headers.set("content-type", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(r#"{"ttl":86400}"#.into()));
    let mut response = Fetch::Request(Request::new_with_init(&url, &init)?)
        .send()
        .await?;
    if response.status_code() != 201 {
        console_error!("TURN credentials: {}", response.status_code());
        return json(STUN.into());
    }
    json(response.text().await?)
}

/// Streams `<prefix>/<file>` from the R2 bucket.
async fn serve_from_r2(req: Request, ctx: RouteContext<()>, prefix: &str) -> Result<Response> {
    let Some(file) = ctx.param("file") else {
        return not_found();
    };
    match from_r2(&req, &ctx, &format!("{prefix}/{file}")).await? {
        Some(response) => Ok(response),
        None => not_found(),
    }
}

/// Streams `key` from the R2 bucket, with the content type it was uploaded with, or `None`
/// when there's no such object. Answers 304 when the browser's copy (If-None-Match) is still
/// current.
async fn from_r2(req: &Request, ctx: &RouteContext<()>, key: &str) -> Result<Option<Response>> {
    let cached_etag = req
        .headers()
        .get("if-none-match")?
        .map(|etag| etag.trim_matches('"').to_string());
    let Some(object) = ctx
        .bucket("BUCKET")?
        .get(key)
        .only_if(Conditional {
            etag_does_not_match: cached_etag,
            ..Default::default()
        })
        .execute()
        .await?
    else {
        return Ok(None);
    };
    // `Headers::clone` copies; share the underlying JS object so the metadata
    // (content-type: application/wasm) lands on the response.
    let headers = Headers::new();
    object.write_http_metadata(Headers(headers.0.clone()))?;
    headers.set("etag", &object.http_etag())?;
    // R2 leaves out the body when the etag matched: the browser's copy is current.
    let Some(body) = object.body() else {
        return Ok(Some(
            Response::empty()?.with_status(304).with_headers(headers),
        ));
    };
    Ok(Some(
        Response::from_body(body.response_body()?)?.with_headers(headers),
    ))
}

/// A 404 browsers must not cache: the file may be uploaded later.
fn not_found() -> Result<Response> {
    let headers = Headers::new();
    headers.set("cache-control", "no-store")?;
    Ok(Response::error("Not found", 404)?.with_headers(headers))
}

/// A bar room: everyone in it sees each other walk around and who plays at which cabinet or
/// table (a place with seats, named "x,y" for a cabinet and "pool:x,y" and the like for a
/// table), and the players at one (up to 4, or `MAX_SEATS` at an arcade game's linked
/// cabinets) find each other here to play online. Anyone else can watch a place's game: one of
/// its players streams it to them through the room. WebSockets go through the Hibernation API,
/// so an idle room is evicted from memory while its connections stay open; what the room knows
/// about each player lives on their socket (its attachment), and each socket is tagged with its
/// player's id.
///
/// Players have a name, shown above their head and next to what they say in the chat.
///
/// Text messages are JSON (`FromPlayer`, `ToPlayer`). Binary messages are for another player,
/// passed on as they are but for the address: `[to: u32 LE][bytes]` in, `[from: u32 LE][bytes]`
/// out. The page sends game packets this way until WebRTC connects, and hands games over. A
/// seated player sending to `WATCHERS`, in binary or as a `Signal`, reaches everyone watching
/// their place, and one sending to `SEVERAL` reaches the players it lists (an arcade game's
/// link data, from each cabinet to all the others it has no direct connection to).
#[durable_object]
pub struct Room {
    state: State,
}

/// What the room knows about a player.
#[derive(Serialize, Deserialize, Clone)]
struct Player {
    id: u32,
    #[serde(default)]
    name: String,
    /// Where their feet are, once they've said.
    at: Option<Position>,
    /// Where they're playing.
    seat: Option<Seat>,
    /// The cabinet ("x,y") whose game they watch.
    #[serde(default)]
    watching: Option<String>,
}

/// The address of everyone watching the sender's cabinet; no player has this id.
const WATCHERS: u32 = 0;
/// The address of a binary message for several players, `[SEVERAL][count: u8][count × id: u32
/// LE][bytes]`: each of them gets `[from][bytes]`, so the sender sends it once. No player has
/// this id.
const SEVERAL: u32 = u32::MAX;
/// The most seats a place has: an arcade game's linked cabinets (Daytona USA's 8). Other games
/// take up to 4 (assets/games.ron), tables 2 to 4.
const MAX_SEATS: usize = 8;

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    flip: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct Seat {
    /// The cabinet's cell, "x,y".
    cabinet: String,
    /// 0 for player 1, and so on.
    index: usize,
    /// How many players the cabinet's game takes.
    of: usize,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FromPlayer {
    Move(Position),
    /// Take a free seat at a cabinet whose game takes `seats` players (2 if not said, at most
    /// `MAX_SEATS`), leaving any other.
    Sit {
        cabinet: String,
        seats: Option<usize>,
    },
    /// Watch the game at a cabinet, leaving any seat.
    Watch {
        cabinet: String,
    },
    /// Leave the seat, or stop watching.
    Stand,
    /// Change the player's name (it's trimmed to 20 characters).
    Name {
        name: String,
    },
    /// Say something to everyone in the room (up to 200 characters).
    Say {
        text: String,
    },
    /// Messages between the players at a cabinet or table (WebRTC offers, answers and ICE
    /// candidates, handing a game over, a table's game), passed on as they are: to another
    /// player, or to everyone watching the sender's place (`WATCHERS`).
    Signal {
        to: u32,
        data: serde_json::Value,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ToPlayer<'a> {
    /// First message: the player's id and name, and everyone else.
    Welcome {
        id: u32,
        name: &'a str,
        players: Vec<PlayerAt>,
        seats: BTreeMap<String, Vec<Option<u32>>>,
        watchers: BTreeMap<String, Vec<u32>>,
    },
    /// Also how a newcomer first shows up, and how a new name spreads.
    Moved(PlayerAt),
    /// A chat message.
    Said {
        id: u32,
        name: &'a str,
        text: &'a str,
    },
    Left {
        id: u32,
    },
    /// Who sits at a cabinet now: a slot per seat, or none when nobody does.
    Seats {
        cabinet: &'a str,
        players: Vec<Option<u32>>,
    },
    /// All seats were taken.
    Full {
        cabinet: &'a str,
    },
    /// Who watches a cabinet's game now.
    Watchers {
        cabinet: &'a str,
        players: Vec<u32>,
    },
    Signal {
        from: u32,
        data: serde_json::Value,
    },
}

#[derive(Serialize)]
struct PlayerAt {
    id: u32,
    name: String,
    #[serde(flatten)]
    at: Position,
}

/// A message for several players, after its `SEVERAL` address: who it is for, and what.
fn several(message: &[u8]) -> Option<(Vec<u32>, &[u8])> {
    let (&count, rest) = message.split_first()?;
    let ids = rest.get(..usize::from(count) * 4)?;
    let recipients = ids
        .as_chunks::<4>()
        .0
        .iter()
        .map(|id| u32::from_le_bytes(*id))
        .collect();
    Some((recipients, &rest[ids.len()..]))
}

/// Text as players may send it: one line without control characters or runs of spaces, at most
/// `max` characters. None when nothing is left.
fn clean(text: &str, max: usize) -> Option<String> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .collect();
    let line: String = words
        .iter()
        .filter(|word| !word.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let line: String = line.chars().take(max).collect();
    let line = line.trim_end();
    (!line.is_empty()).then(|| line.to_string())
}

impl Room {
    fn players(&self) -> Vec<(WebSocket, Player)> {
        self.state
            .get_websockets()
            .into_iter()
            .filter_map(|ws| {
                let player = ws.deserialize_attachment::<Player>().ok()??;
                Some((ws, player))
            })
            .collect()
    }

    fn socket_of(&self, id: u32) -> Option<WebSocket> {
        let mut sockets = self.state.get_websockets_with_tag(&id.to_string());
        sockets.pop()
    }

    fn broadcast(&self, message: &ToPlayer) -> Result<()> {
        let text = serde_json::to_string(message)?;
        for ws in self.state.get_websockets() {
            // A socket that is closing can't take it; the others still should.
            let _ = ws.send_with_str(&text);
        }
        Ok(())
    }

    /// Who sits at `cabinet`, leaving out the player `except` (who is leaving).
    fn seats_at(&self, cabinet: &str, except: u32) -> Vec<Option<u32>> {
        let seated: Vec<(Seat, u32)> = self
            .players()
            .into_iter()
            .filter_map(|(_, player)| Some((player.seat?, player.id)))
            .filter(|(seat, id)| seat.cabinet == cabinet && *id != except)
            .collect();
        let size = seated.iter().map(|(seat, _)| seat.of).max().unwrap_or(0);
        let mut seats = vec![None; size];
        for (seat, id) in seated {
            seats[seat.index] = Some(id);
        }
        seats
    }

    /// Who watches `cabinet`, leaving out the player `except` (who is leaving).
    fn watchers_of(&self, cabinet: &str, except: u32) -> Vec<u32> {
        self.players()
            .into_iter()
            .filter(|(_, player)| player.id != except)
            .filter(|(_, player)| player.watching.as_deref() == Some(cabinet))
            .map(|(_, player)| player.id)
            .collect()
    }

    /// Frees the player's seat or stops them watching, and tells everyone.
    fn stand(&self, ws: &WebSocket, player: &mut Player) -> Result<()> {
        let seat = player.seat.take();
        let watching = player.watching.take();
        if seat.is_none() && watching.is_none() {
            return Ok(());
        }
        ws.serialize_attachment(&*player)?;
        if let Some(seat) = seat {
            let players = self.seats_at(&seat.cabinet, player.id);
            let cabinet = &seat.cabinet;
            self.broadcast(&ToPlayer::Seats { cabinet, players })?;
        }
        if let Some(cabinet) = watching {
            let players = self.watchers_of(&cabinet, player.id);
            let cabinet = &cabinet;
            self.broadcast(&ToPlayer::Watchers { cabinet, players })?;
        }
        Ok(())
    }

    fn watch(&self, ws: &WebSocket, player: &mut Player, cabinet: String) -> Result<()> {
        if player.watching.as_ref() == Some(&cabinet) {
            return Ok(());
        }
        self.stand(ws, player)?;
        player.watching = Some(cabinet.clone());
        ws.serialize_attachment(&*player)?;
        let players = self.watchers_of(&cabinet, 0);
        self.broadcast(&ToPlayer::Watchers {
            cabinet: &cabinet,
            players,
        })
    }

    fn sit(&self, ws: &WebSocket, player: &mut Player, cabinet: String, of: usize) -> Result<()> {
        if player
            .seat
            .as_ref()
            .is_some_and(|seat| seat.cabinet == cabinet)
        {
            return Ok(());
        }
        self.stand(ws, player)?;
        let mut players = self.seats_at(&cabinet, player.id);
        players.resize(players.len().max(of), None);
        let Some(index) = players.iter().position(Option::is_none) else {
            return ws.send(&ToPlayer::Full { cabinet: &cabinet });
        };
        players[index] = Some(player.id);
        let of = players.len();
        player.seat = Some(Seat {
            cabinet: cabinet.clone(),
            index,
            of,
        });
        ws.serialize_attachment(&*player)?;
        self.broadcast(&ToPlayer::Seats {
            cabinet: &cabinet,
            players,
        })
    }

    /// The player's socket closed: free their seat and tell everyone they left.
    fn leave(&self, ws: &WebSocket) -> Result<()> {
        let Some(player) = ws.deserialize_attachment::<Player>()? else {
            return Ok(());
        };
        if let Some(seat) = &player.seat {
            let players = self.seats_at(&seat.cabinet, player.id);
            let cabinet = &seat.cabinet;
            self.broadcast(&ToPlayer::Seats { cabinet, players })?;
        }
        if let Some(cabinet) = &player.watching {
            let players = self.watchers_of(cabinet, player.id);
            self.broadcast(&ToPlayer::Watchers { cabinet, players })?;
        }
        self.broadcast(&ToPlayer::Left { id: player.id })
    }

    /// Passes on a binary message from `player`: to another player, to several (`SEVERAL`), or
    /// to everyone watching the player's cabinet.
    fn pass_on(&self, player: &Player, mut message: Vec<u8>) -> Result<()> {
        if message.len() < 4 {
            return Ok(());
        }
        let to = u32::from_le_bytes([message[0], message[1], message[2], message[3]]);
        if to == SEVERAL {
            let Some((recipients, bytes)) = several(&message[4..]) else {
                return Ok(());
            };
            let mut out = Vec::with_capacity(4 + bytes.len());
            out.extend_from_slice(&player.id.to_le_bytes());
            out.extend_from_slice(bytes);
            for id in recipients {
                if let Some(other) = self.socket_of(id) {
                    // A socket that is closing can't take it; the others still should.
                    let _ = other.send_with_bytes(&out);
                }
            }
            return Ok(());
        }
        message[..4].copy_from_slice(&player.id.to_le_bytes());
        if to != WATCHERS {
            if let Some(other) = self.socket_of(to) {
                other.send_with_bytes(message)?;
            }
            return Ok(());
        }
        for ws in self.watchers_sockets(player) {
            // A socket that is closing can't take it; the others still should.
            let _ = ws.send_with_bytes(&message);
        }
        Ok(())
    }

    /// Passes on a `Signal` from `player`: to another player, or to everyone watching the
    /// player's cabinet or table.
    fn signal(&self, player: &Player, to: u32, data: serde_json::Value) -> Result<()> {
        let message = ToPlayer::Signal {
            from: player.id,
            data,
        };
        if to != WATCHERS {
            if let Some(other) = self.socket_of(to) {
                other.send(&message)?;
            }
            return Ok(());
        }
        let text = serde_json::to_string(&message)?;
        for ws in self.watchers_sockets(player) {
            let _ = ws.send_with_str(&text);
        }
        Ok(())
    }

    /// The sockets of everyone watching the place `player` sits at; none if they sit nowhere.
    fn watchers_sockets(&self, player: &Player) -> Vec<WebSocket> {
        let Some(seat) = &player.seat else {
            return Vec::new();
        };
        self.players()
            .into_iter()
            .filter(|(_, other)| other.watching.as_ref() == Some(&seat.cabinet))
            .map(|(ws, _)| ws)
            .collect()
    }
}

impl DurableObject for Room {
    fn new(state: State, _env: Env) -> Self {
        Self { state }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        if req.headers().get("Upgrade")?.as_deref() != Some("websocket") {
            return Response::error("Expected a WebSocket upgrade", 426);
        }
        let others = self.players();
        let id = loop {
            let id = (js_sys::Math::random() * u32::MAX as f64) as u32;
            if id != WATCHERS && id != SEVERAL && !others.iter().any(|(_, player)| player.id == id)
            {
                break id;
            }
        };
        let pair = WebSocketPair::new()?;
        let tag = id.to_string();
        self.state.accept_websocket_with_tags(&pair.server, &[&tag]);
        let player = Player {
            id,
            name: format!("Guest {:04}", id % 10000),
            at: None,
            seat: None,
            watching: None,
        };
        pair.server.serialize_attachment(&player)?;

        let mut seats = BTreeMap::<String, Vec<Option<u32>>>::new();
        let mut watchers = BTreeMap::<String, Vec<u32>>::new();
        for (_, other) in &others {
            if let Some(seat) = &other.seat {
                let players = seats.entry(seat.cabinet.clone()).or_default();
                players.resize(players.len().max(seat.of), None);
                players[seat.index] = Some(other.id);
            }
            if let Some(cabinet) = &other.watching {
                watchers.entry(cabinet.clone()).or_default().push(other.id);
            }
        }
        let players = others
            .iter()
            .filter_map(|(_, other)| {
                Some(PlayerAt {
                    id: other.id,
                    name: other.name.clone(),
                    at: other.at?,
                })
            })
            .collect();
        pair.server.send(&ToPlayer::Welcome {
            id,
            name: &player.name,
            players,
            seats,
            watchers,
        })?;
        Response::from_websocket(pair.client)
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let Some(mut player) = ws.deserialize_attachment::<Player>()? else {
            return Ok(());
        };
        let text = match message {
            WebSocketIncomingMessage::String(text) => text,
            WebSocketIncomingMessage::Binary(bytes) => return self.pass_on(&player, bytes),
        };
        match serde_json::from_str::<FromPlayer>(&text)? {
            FromPlayer::Move(at) => {
                player.at = Some(at);
                ws.serialize_attachment(&player)?;
                let name = player.name;
                self.broadcast(&ToPlayer::Moved(PlayerAt {
                    id: player.id,
                    name,
                    at,
                }))?;
            }
            FromPlayer::Name { name } => {
                let Some(name) = clean(&name, 20) else {
                    return Ok(());
                };
                player.name = name.clone();
                ws.serialize_attachment(&player)?;
                // Others see it once the player is somewhere.
                if let Some(at) = player.at {
                    self.broadcast(&ToPlayer::Moved(PlayerAt {
                        id: player.id,
                        name,
                        at,
                    }))?;
                }
            }
            FromPlayer::Say { text } => {
                if let Some(text) = clean(&text, 200) {
                    let name = &player.name;
                    self.broadcast(&ToPlayer::Said {
                        id: player.id,
                        name,
                        text: &text,
                    })?;
                }
            }
            FromPlayer::Sit { cabinet, seats } => {
                let of = seats.unwrap_or(2).clamp(1, MAX_SEATS);
                self.sit(&ws, &mut player, cabinet, of)?;
            }
            FromPlayer::Watch { cabinet } => self.watch(&ws, &mut player, cabinet)?,
            FromPlayer::Stand => self.stand(&ws, &mut player)?,
            FromPlayer::Signal { to, data } => self.signal(&player, to, data)?,
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        code: usize,
        reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        self.leave(&ws)?;
        // Complete the close handshake, as in Cloudflare's hibernation example. 1005 and 1006
        // mean the client gave no code, and can't be sent back.
        let code = if matches!(code, 1005 | 1006) {
            1000
        } else {
            code as u16
        };
        ws.close(Some(code), Some(reason))
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        self.leave(&ws)
    }
}
