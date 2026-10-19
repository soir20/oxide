use std::fmt;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::Deserialize;

// Responses are tiny JSON objects; anything larger is not from the backend
const MAX_RESPONSE_BYTES: u64 = 16 * 1024;

/// Optional `ticket_validation` section of `server.yaml`. Without it, any client
/// may log in as any GUID (the default, for development).
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketValidationOptions {
    /// Plain HTTP endpoint that redeems tickets, e.g. `http://127.0.0.1:8081/internal/game/redeem`
    pub redeem_url: String,
    /// Name of the environment variable holding the shared secret sent as a bearer token
    pub secret_env: String,
    pub timeout_millis: u64,
}

impl TicketValidationOptions {
    pub fn validate(&self) -> Result<(), String> {
        HttpEndpoint::parse(&self.redeem_url)?;
        if self.timeout_millis == 0 {
            return Err("ticket_validation.timeout_millis must be greater than 0".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum TicketError {
    /// The validator refused the ticket. The reason is safe to log.
    Rejected(String),
    /// The validator could not be reached or answered unexpectedly. Logins fail closed.
    Unavailable(String),
}

impl fmt::Display for TicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TicketError::Rejected(reason) => write!(f, "ticket rejected ({reason})"),
            TicketError::Unavailable(reason) => write!(f, "ticket validator unavailable: {reason}"),
        }
    }
}

pub enum TicketValidator {
    Disabled,
    Http(HttpTicketValidator),
}

impl TicketValidator {
    pub fn from_options(options: Option<&TicketValidationOptions>) -> Result<Self, String> {
        let Some(options) = options else {
            return Ok(TicketValidator::Disabled);
        };
        let secret = std::env::var(&options.secret_env)
            .map_err(|_| format!("environment variable {} is not set", options.secret_env))?;
        Ok(TicketValidator::Http(HttpTicketValidator {
            endpoint: HttpEndpoint::parse(&options.redeem_url)?,
            secret,
            timeout: Duration::from_millis(options.timeout_millis),
        }))
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self, TicketValidator::Http(_))
    }

    /// Consumes `ticket` for `guid`. Blocks for at most about twice the configured timeout.
    pub fn validate(&self, ticket: &str, guid: u32) -> Result<(), TicketError> {
        match self {
            TicketValidator::Disabled => Ok(()),
            TicketValidator::Http(validator) => validator.validate(ticket, guid),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct HttpEndpoint {
    host: String,
    port: u16,
    path: String,
}

impl HttpEndpoint {
    fn parse(url: &str) -> Result<Self, String> {
        let rest = url.strip_prefix("http://").ok_or_else(|| {
            format!("ticket_validation.redeem_url must start with http://: {url}")
        })?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| format!("ticket_validation.redeem_url must include a port: {url}"))?;
        let port = port
            .parse()
            .map_err(|_| format!("Invalid port in ticket_validation.redeem_url: {url}"))?;
        if host.is_empty() {
            return Err(format!(
                "Missing host in ticket_validation.redeem_url: {url}"
            ));
        }
        Ok(HttpEndpoint {
            host: host.to_string(),
            port,
            path: path.to_string(),
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

pub struct HttpTicketValidator {
    endpoint: HttpEndpoint,
    secret: String,
    timeout: Duration,
}

#[derive(Deserialize)]
struct Rejection {
    reason: String,
}

impl HttpTicketValidator {
    fn validate(&self, ticket: &str, guid: u32) -> Result<(), TicketError> {
        let (status, body) = self
            .post(&serde_json::json!({ "ticket": ticket, "guid": guid }).to_string())
            .map_err(TicketError::Unavailable)?;
        match status {
            200 => Ok(()),
            403 => Err(TicketError::Rejected(
                serde_json::from_slice::<Rejection>(&body)
                    .map(|rejection| rejection.reason)
                    .unwrap_or_else(|_| "no reason given".to_string()),
            )),
            401 => Err(TicketError::Unavailable(
                "validator refused the shared secret".to_string(),
            )),
            status => Err(TicketError::Unavailable(format!("HTTP status {status}"))),
        }
    }

    /// A minimal HTTP/1.1 POST with `Connection: close`. Returns the status code and body.
    fn post(&self, body: &str) -> Result<(u16, Vec<u8>), String> {
        let addr = self.endpoint.socket_addr()?;
        let mut stream = TcpStream::connect_timeout(&addr, self.timeout)
            .map_err(|err| format!("cannot connect to {addr}: {err}"))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|err| err.to_string())?;

        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.endpoint.path,
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

    /// Serves one canned response and returns the raw request it received.
    fn serve_once(response: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/internal/game/redeem",
            listener.local_addr().unwrap()
        );
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

    fn validator(url: &str) -> HttpTicketValidator {
        HttpTicketValidator {
            endpoint: HttpEndpoint::parse(url).unwrap(),
            secret: SECRET.to_string(),
            timeout: Duration::from_millis(500),
        }
    }

    #[test]
    fn accepts_on_200_and_sends_the_ticket_guid_and_secret() {
        let (url, request) = serve_once(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n{}",
        );
        assert_eq!(validator(&url).validate("p7w9dGPB-_", 4102479591), Ok(()));
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /internal/game/redeem HTTP/1.1\r\n"));
        assert!(request.contains(&format!("\r\nAuthorization: Bearer {SECRET}\r\n")));
        let body: serde_json::Value =
            serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({ "ticket": "p7w9dGPB-_", "guid": 4102479591u32 })
        );
    }

    #[test]
    fn json_escapes_the_ticket() {
        let (url, request) = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
        assert_eq!(validator(&url).validate("a\"b\\c", 1), Ok(()));
        assert!(request.recv().unwrap().contains(r#""ticket":"a\"b\\c""#));
    }

    #[test]
    fn rejects_on_403_with_the_reason() {
        let (url, _request) = serve_once(
            "HTTP/1.1 403 Forbidden\r\ncontent-length: 51\r\n\r\n{\"error\":\"ticket_rejected\",\"reason\":\"already_used\"}",
        );
        assert_eq!(
            validator(&url).validate("t", 1),
            Err(TicketError::Rejected("already_used".to_string()))
        );
    }

    #[test]
    fn fails_closed_on_other_statuses() {
        for response in [
            "HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n",
            "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\r\n",
            "garbage",
        ] {
            let (url, _request) = serve_once(response);
            assert!(matches!(
                validator(&url).validate("t", 1),
                Err(TicketError::Unavailable(_))
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
        let url = format!("http://127.0.0.1:{port}/internal/game/redeem");
        assert!(matches!(
            validator(&url).validate("t", 1),
            Err(TicketError::Unavailable(_))
        ));
    }

    #[test]
    fn fails_closed_when_the_validator_hangs() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/redeem", listener.local_addr().unwrap());
        // Accepts the connection but never answers
        let _hold = thread::spawn(move || listener.accept().map(|(stream, _)| stream));
        assert!(matches!(
            validator(&url).validate("t", 1),
            Err(TicketError::Unavailable(_))
        ));
    }

    #[test]
    fn disabled_accepts_everything() {
        assert_eq!(TicketValidator::Disabled.validate("", 1), Ok(()));
    }

    #[test]
    fn parses_endpoints() {
        assert_eq!(
            HttpEndpoint::parse("http://127.0.0.1:8081/internal/game/redeem"),
            Ok(HttpEndpoint {
                host: "127.0.0.1".to_string(),
                port: 8081,
                path: "/internal/game/redeem".to_string()
            })
        );
        assert_eq!(
            HttpEndpoint::parse("http://localhost:80").unwrap().path,
            "/"
        );
        for url in [
            "https://127.0.0.1:8081/x",
            "http://127.0.0.1/x",
            "http://:8081/x",
            "http://127.0.0.1:port/x",
        ] {
            assert!(HttpEndpoint::parse(url).is_err(), "{url}");
        }
    }

    #[test]
    fn error_messages_do_not_contain_the_ticket() {
        let (url, _request) = serve_once("HTTP/1.1 403 Forbidden\r\ncontent-length: 2\r\n\r\n{}");
        let err = validator(&url).validate("SECRET-TICKET", 1).unwrap_err();
        assert!(!err.to_string().contains("SECRET-TICKET"));
    }
}
