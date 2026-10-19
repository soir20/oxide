use std::fmt;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::{Deserialize, Serialize};

// Responses are small JSON objects; anything larger is not from the backend
const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

/// Optional `backend` section of `server.yaml`. Without it, any client may log in as any
/// GUID and nothing is saved (the default, for development).
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendOptions {
    /// Base URL of the backend's internal API over plain HTTP, e.g. `http://127.0.0.1:8081`
    pub url: String,
    /// Name of the environment variable holding the shared secret sent as a bearer token
    pub secret_env: String,
    pub timeout_millis: u64,
    /// How often the characters of online players are saved, in addition to on logout
    pub save_period_millis: u64,
}

impl BackendOptions {
    pub fn validate(&self) -> Result<(), String> {
        HttpEndpoint::parse(&self.url)?;
        if self.timeout_millis == 0 {
            return Err("backend.timeout_millis must be greater than 0".to_string());
        }
        if self.save_period_millis == 0 {
            return Err("backend.save_period_millis must be greater than 0".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum BackendError {
    /// The backend refused the ticket. The reason is safe to log.
    Rejected(String),
    /// The backend could not be reached or answered unexpectedly. Logins fail closed.
    Unavailable(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::Rejected(reason) => write!(f, "ticket rejected ({reason})"),
            BackendError::Unavailable(reason) => write!(f, "backend unavailable: {reason}"),
        }
    }
}

/// The account and character a redeemed ticket belongs to.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Redeemed {
    pub account: Account,
    pub character: SavedCharacter,
}

/// Read at every login, so a change takes effect at the next one.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Account {
    pub role: AccountRole,
    /// `None` unless a membership is in effect
    pub membership: Option<Membership>,
}

impl Account {
    pub fn is_member(&self) -> bool {
        self.membership.is_some()
    }
}

/// An unknown role fails the login rather than guessing its permissions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AccountRole {
    Member,
    Counselor,
    Enforcer,
    Admin,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Membership {
    #[serde(rename = "type")]
    pub membership_type: String,
    pub ends_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct SavedCharacter {
    pub name: String,
    /// `None` until the character is saved for the first time
    pub state: Option<CharacterSnapshot>,
}

/// Everything about a character that survives a logout. Field meanings are documented in the
/// CWA project's `docs/DATABASE.md`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterSnapshot {
    pub zone_template: u8,
    pub pos: [f32; 4],
    pub rot: [f32; 4],
    pub credits: u32,
    pub active_battle_class: u32,
    pub equipment: Vec<SavedEquippedItem>,
    pub customizations: Vec<SavedCustomization>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedEquippedItem {
    pub battle_class: u32,
    pub slot: u32,
    pub item_guid: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedCustomization {
    pub slot: i32,
    pub customization_id: i32,
}

pub enum Backend {
    Disabled,
    Http(HttpBackend),
}

impl Backend {
    pub fn from_options(options: Option<&BackendOptions>) -> Result<Self, String> {
        let Some(options) = options else {
            return Ok(Backend::Disabled);
        };
        let secret = std::env::var(&options.secret_env)
            .map_err(|_| format!("environment variable {} is not set", options.secret_env))?;
        Ok(Backend::Http(HttpBackend {
            endpoint: HttpEndpoint::parse(&options.url)?,
            secret,
            timeout: Duration::from_millis(options.timeout_millis),
            save_period: Duration::from_millis(options.save_period_millis),
        }))
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self, Backend::Http(_))
    }

    pub fn save_period(&self) -> Option<Duration> {
        match self {
            Backend::Disabled => None,
            Backend::Http(backend) => Some(backend.save_period),
        }
    }

    /// Consumes `ticket` for `guid` and returns the account and character to load, or `None`
    /// when the backend is disabled. Blocks for at most about twice the configured timeout.
    pub fn redeem(&self, ticket: &str, guid: u32) -> Result<Option<Redeemed>, BackendError> {
        match self {
            Backend::Disabled => Ok(None),
            Backend::Http(backend) => backend.redeem(ticket, guid).map(Some),
        }
    }

    pub fn save(&self, guid: u32, snapshot: &CharacterSnapshot) -> Result<(), String> {
        match self {
            Backend::Disabled => Ok(()),
            Backend::Http(backend) => backend.save(guid, snapshot),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct HttpEndpoint {
    host: String,
    port: u16,
    base_path: String,
}

impl HttpEndpoint {
    fn parse(url: &str) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("backend.url must start with http://: {url}"))?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| format!("backend.url must include a port: {url}"))?;
        let port = port
            .parse()
            .map_err(|_| format!("Invalid port in backend.url: {url}"))?;
        if host.is_empty() {
            return Err(format!("Missing host in backend.url: {url}"));
        }
        Ok(HttpEndpoint {
            host: host.to_string(),
            port,
            base_path: path.trim_end_matches('/').to_string(),
        })
    }

    fn socket_addr(&self) -> Result<SocketAddr, String> {
        let host = self.host.trim_start_matches('[').trim_end_matches(']');
        (host, self.port)
            .to_socket_addrs()
            .map_err(|err| format!("cannot resolve {}: {err}", self.host))?
            .next()
            .ok_or_else(|| format!("no address for {}", self.host))
    }
}

pub struct HttpBackend {
    endpoint: HttpEndpoint,
    secret: String,
    timeout: Duration,
    save_period: Duration,
}

#[derive(Deserialize)]
struct Rejection {
    reason: String,
}

impl HttpBackend {
    fn redeem(&self, ticket: &str, guid: u32) -> Result<Redeemed, BackendError> {
        let body = serde_json::json!({ "ticket": ticket, "guid": guid }).to_string();
        let (status, body) = self
            .request("POST", "/internal/game/redeem", &body)
            .map_err(BackendError::Unavailable)?;
        match status {
            200 => serde_json::from_slice::<Redeemed>(&body).map_err(|err| {
                BackendError::Unavailable(format!("invalid redeem response: {err}"))
            }),
            403 => Err(BackendError::Rejected(
                serde_json::from_slice::<Rejection>(&body)
                    .map(|rejection| rejection.reason)
                    .unwrap_or_else(|_| "no reason given".to_string()),
            )),
            401 => Err(BackendError::Unavailable(
                "backend refused the shared secret".to_string(),
            )),
            status => Err(BackendError::Unavailable(format!("HTTP status {status}"))),
        }
    }

    fn save(&self, guid: u32, snapshot: &CharacterSnapshot) -> Result<(), String> {
        let body = serde_json::to_string(snapshot).map_err(|err| err.to_string())?;
        let (status, body) = self.request(
            "PUT",
            &format!("/internal/game/characters/{guid}/state"),
            &body,
        )?;
        match status {
            204 => Ok(()),
            status => Err(format!(
                "HTTP status {status}: {}",
                String::from_utf8_lossy(&body[..body.len().min(200)])
            )),
        }
    }

    /// A minimal HTTP/1.1 request with `Connection: close`. Returns the status code and body.
    fn request(&self, method: &str, path: &str, body: &str) -> Result<(u16, Vec<u8>), String> {
        let addr = self.endpoint.socket_addr()?;
        let mut stream = TcpStream::connect_timeout(&addr, self.timeout)
            .map_err(|err| format!("cannot connect to {addr}: {err}"))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|err| err.to_string())?;

        let request = format!(
            "{method} {}{path} HTTP/1.1\r\nHost: {}:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.endpoint.base_path,
            self.endpoint.host,
            self.endpoint.port,
            self.secret,
            body.len(),
            body
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|err| format!("cannot send request: {err}"))?;

        let mut response = Vec::new();
        stream
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut response)
            .map_err(|err| format!("cannot read response: {err}"))?;
        parse_response(&response)
    }
}

fn parse_response(response: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("incomplete HTTP response")?;
    let head = std::str::from_utf8(&response[..header_end]).map_err(|_| "invalid HTTP header")?;
    let status_line = head.lines().next().unwrap_or_default();
    let mut parts = status_line.split(' ');
    if !parts.next().unwrap_or_default().starts_with("HTTP/1.") {
        return Err(format!("unexpected status line: {status_line}"));
    }
    let status = parts
        .next()
        .and_then(|status| status.parse().ok())
        .ok_or_else(|| format!("unexpected status line: {status_line}"))?;
    if head
        .lines()
        .any(|line| line.to_ascii_lowercase().starts_with("transfer-encoding:"))
    {
        return Err("chunked responses are not supported".to_string());
    }
    Ok((status, response[header_end + 4..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    const SECRET: &str = "test-secret-0123456789abcdef0123456789";
    const REDEEMED: &str = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"user_id\":1,\"profile_id\":2,\"guid\":4102479591,\"account\":{\"role\":\"member\",\"membership\":null},\"character\":{\"name\":\"Rex\",\"state\":null}}";

    /// Serves one canned response and returns the raw request it received.
    fn serve_once(response: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length: usize = head
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            stream.write_all(response.as_bytes()).unwrap();
            sender.send(String::from_utf8(request).unwrap()).unwrap();
        });
        (url, receiver)
    }

    fn backend(url: &str) -> HttpBackend {
        HttpBackend {
            endpoint: HttpEndpoint::parse(url).unwrap(),
            secret: SECRET.to_string(),
            timeout: Duration::from_millis(500),
            save_period: Duration::from_secs(60),
        }
    }

    fn body(request: &str) -> serde_json::Value {
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    }

    fn snapshot() -> CharacterSnapshot {
        CharacterSnapshot {
            zone_template: 24,
            pos: [887.3, 171.93376, 1546.956, 1.0],
            rot: [1.5, 0.0, 0.0, 0.0],
            credits: 999_250,
            active_battle_class: 1,
            equipment: vec![SavedEquippedItem {
                battle_class: 1,
                slot: 1,
                item_guid: 259,
            }],
            customizations: vec![SavedCustomization {
                slot: 0,
                customization_id: 110000,
            }],
        }
    }

    #[test]
    fn redeem_sends_the_ticket_guid_and_secret() {
        let (url, request) = serve_once(REDEEMED);
        let redeemed = backend(&url).redeem("p7w9dGPB-_", 4102479591).unwrap();
        assert_eq!(
            redeemed,
            Redeemed {
                account: Account {
                    role: AccountRole::Member,
                    membership: None,
                },
                character: SavedCharacter {
                    name: "Rex".to_string(),
                    state: None
                },
            }
        );
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /internal/game/redeem HTTP/1.1\r\n"));
        assert!(request.contains(&format!("\r\nAuthorization: Bearer {SECRET}\r\n")));
        assert_eq!(
            body(&request),
            serde_json::json!({ "ticket": "p7w9dGPB-_", "guid": 4102479591u32 })
        );
    }

    #[test]
    fn redeem_returns_the_saved_state() {
        let (url, _request) = serve_once("HTTP/1.1 200 OK\r\n\r\n{\"user_id\":1,\"profile_id\":2,\"guid\":3,\"account\":{\"role\":\"member\",\"membership\":null},\"character\":{\"name\":\"Rex\",\"state\":{\"zone_template\":24,\"pos\":[887.3,171.93376,1546.956,1.0],\"rot\":[1.5,0.0,0.0,0.0],\"credits\":999250,\"active_battle_class\":1,\"equipment\":[{\"battle_class\":1,\"slot\":1,\"item_guid\":259}],\"customizations\":[{\"slot\":0,\"customization_id\":110000}]}}}");
        let redeemed = backend(&url).redeem("t", 3).unwrap();
        assert_eq!(redeemed.character.state, Some(snapshot()));
    }

    #[test]
    fn redeem_returns_the_role_and_membership() {
        let (url, _request) = serve_once("HTTP/1.1 200 OK\r\n\r\n{\"user_id\":1,\"profile_id\":2,\"guid\":3,\"account\":{\"role\":\"enforcer\",\"membership\":{\"type\":\"standard\",\"starts_at\":1,\"ends_at\":null}},\"character\":{\"name\":\"Rex\",\"state\":null}}");
        let account = backend(&url).redeem("t", 3).unwrap().account;
        assert_eq!(account.role, AccountRole::Enforcer);
        assert!(account.is_member());
        assert_eq!(account.membership.unwrap().membership_type, "standard");
    }

    #[test]
    fn an_unknown_role_fails_closed() {
        let (url, _request) = serve_once("HTTP/1.1 200 OK\r\n\r\n{\"user_id\":1,\"profile_id\":2,\"guid\":3,\"account\":{\"role\":\"emperor\",\"membership\":null},\"character\":{\"name\":\"Rex\",\"state\":null}}");
        assert!(matches!(
            backend(&url).redeem("t", 3),
            Err(BackendError::Unavailable(_))
        ));
    }

    #[test]
    fn json_escapes_the_ticket() {
        let (url, request) = serve_once(REDEEMED);
        backend(&url).redeem("a\"b\\c", 1).unwrap();
        assert!(request.recv().unwrap().contains(r#""ticket":"a\"b\\c""#));
    }

    #[test]
    fn rejects_on_403_with_the_reason() {
        let (url, _request) = serve_once(
            "HTTP/1.1 403 Forbidden\r\ncontent-length: 51\r\n\r\n{\"error\":\"ticket_rejected\",\"reason\":\"already_used\"}",
        );
        assert_eq!(
            backend(&url).redeem("t", 1),
            Err(BackendError::Rejected("already_used".to_string()))
        );
    }

    #[test]
    fn fails_closed_on_other_statuses_and_bad_bodies() {
        for response in [
            "HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n",
            "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\n\r\n{}",
            "garbage",
        ] {
            let (url, _request) = serve_once(response);
            assert!(matches!(
                backend(&url).redeem("t", 1),
                Err(BackendError::Unavailable(_))
            ));
        }
    }

    #[test]
    fn fails_closed_when_nothing_listens() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("http://127.0.0.1:{port}");
        assert!(matches!(
            backend(&url).redeem("t", 1),
            Err(BackendError::Unavailable(_))
        ));
    }

    #[test]
    fn fails_closed_when_the_backend_hangs() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        // Accepts the connection but never answers
        let _hold = thread::spawn(move || listener.accept().map(|(stream, _)| stream));
        assert!(matches!(
            backend(&url).redeem("t", 1),
            Err(BackendError::Unavailable(_))
        ));
    }

    #[test]
    fn save_puts_the_snapshot() {
        let (url, request) = serve_once("HTTP/1.1 204 No Content\r\n\r\n");
        assert_eq!(backend(&format!("{url}/")).save(42, &snapshot()), Ok(()));
        let request = request.recv().unwrap();
        assert!(request.starts_with("PUT /internal/game/characters/42/state HTTP/1.1\r\n"));
        assert!(request.contains(&format!("\r\nAuthorization: Bearer {SECRET}\r\n")));
        let saved: CharacterSnapshot = serde_json::from_value(body(&request)).unwrap();
        assert_eq!(saved, snapshot());
    }

    #[test]
    fn save_reports_failures() {
        let (url, _request) = serve_once(
            "HTTP/1.1 404 Not Found\r\n\r\n{\"error\":\"not_found\",\"message\":\"not found\"}",
        );
        let err = backend(&url).save(42, &snapshot()).unwrap_err();
        assert!(err.starts_with("HTTP status 404"), "{err}");
    }

    #[test]
    fn disabled_accepts_everything_and_saves_nothing() {
        assert_eq!(Backend::Disabled.redeem("", 1), Ok(None));
        assert_eq!(Backend::Disabled.save(1, &snapshot()), Ok(()));
        assert_eq!(Backend::Disabled.save_period(), None);
    }

    #[test]
    fn parses_endpoints() {
        assert_eq!(
            HttpEndpoint::parse("http://127.0.0.1:8081"),
            Ok(HttpEndpoint {
                host: "127.0.0.1".to_string(),
                port: 8081,
                base_path: "".to_string()
            })
        );
        assert_eq!(
            HttpEndpoint::parse("http://localhost:80/cwa/")
                .unwrap()
                .base_path,
            "/cwa"
        );
        for url in [
            "https://127.0.0.1:8081",
            "http://127.0.0.1",
            "http://:8081",
            "http://127.0.0.1:port",
        ] {
            assert!(HttpEndpoint::parse(url).is_err(), "{url}");
        }
    }

    #[test]
    fn error_messages_do_not_contain_the_ticket() {
        let (url, _request) = serve_once("HTTP/1.1 403 Forbidden\r\ncontent-length: 2\r\n\r\n{}");
        let err = backend(&url).redeem("SECRET-TICKET", 1).unwrap_err();
        assert!(!err.to_string().contains("SECRET-TICKET"));
    }
}
