//! Hyperlinks panel ▸ Check URL Status (build-c2): whether the web addresses of URL destinations answer, asked only when the person
//! (or an MCP client) asks for it — this uses the network. Each address gets an HTTP `HEAD` (a `GET` that reads only the headers when
//! the server refuses `HEAD`), redirections are followed up to `MAX_REDIRECTS`, each address has a time limit. The answer is a
//! structured value per address: the status code and a kind (`ok`, `notFound`, `clientError`, `serverError`, `tooManyRedirects`,
//! `timeout`, `dns`, `connect`, `tls`, `invalid`, `notChecked` for addresses that are not http/https such as `mailto:`).
//! No new dependency: std's TCP and the rustls (ring) + webpki-roots the database connections already use. Nothing is stored.
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

const MAX_REDIRECTS: usize = 5;
const MAX_URLS: usize = 200;
const PARALLEL: usize = 8;
const HEADER_LIMIT: usize = 64 * 1024;

#[derive(Debug, PartialEq)]
struct Url {
    https: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse(url: &str) -> Result<Url, &'static str> {
    let url = url.trim();
    let (https, rest) = if let Some(r) = url.strip_prefix("https://").or_else(|| url.strip_prefix("HTTPS://")) { (true, r) }
        else if let Some(r) = url.strip_prefix("http://").or_else(|| url.strip_prefix("HTTP://")) { (false, r) }
        else { return Err(if url.contains(':') { "notChecked" } else { "invalid" }) };
    let rest = rest.split('#').next().unwrap_or("");
    let (authority, path) = match rest.find(['/', '?']) { Some(i) => (&rest[..i], &rest[i..]), None => (rest, "/") };
    let path = if path.starts_with('?') { format!("/{path}") } else { path.to_owned() };
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or("invalid")?;
        (h.to_owned(), after.strip_prefix(':').map(|p| p.parse().map_err(|_| "invalid")).transpose()?)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_owned(), Some(p.parse().map_err(|_| "invalid")?)),
            None => (authority.to_owned(), None),
        }
    };
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) || path.chars().any(|c| c.is_whitespace() || c.is_control()) { return Err("invalid"); }
    Ok(Url { https, port: port.unwrap_or(if https { 443 } else { 80 }), host, path })
}

/// The address a `Location` header points at, from the address that answered.
fn resolve(base: &Url, location: &str) -> String {
    let scheme = if base.https { "https" } else { "http" };
    let authority = if (base.https && base.port == 443) || (!base.https && base.port == 80) { base.host.clone() } else { format!("{}:{}", base.host, base.port) };
    if location.starts_with("http://") || location.starts_with("https://") { location.to_owned() }
    else if let Some(rest) = location.strip_prefix("//") { format!("{scheme}://{rest}") }
    else if location.starts_with('/') { format!("{scheme}://{authority}{location}") }
    else {
        let dir = base.path.split('?').next().unwrap_or("/").rsplit_once('/').map_or("", |(d, _)| d);
        format!("{scheme}://{authority}{dir}/{location}")
    }
}

fn tls() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions()
            .expect("ring supports the default TLS versions").with_root_certificates(roots).with_no_client_auth();
        Arc::new(config)
    }).clone()
}

/// What went wrong, as a kind.
fn kind_of(e: &std::io::Error) -> &'static str {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => "timeout",
        std::io::ErrorKind::InvalidData => "tls",
        _ if e.get_ref().is_some_and(|inner| inner.is::<rustls::Error>()) => "tls",
        _ => "connect",
    }
}

/// One request; the status code and the `Location` header.
fn ask(url: &Url, method: &str, deadline: Instant) -> Result<(u16, Option<String>), &'static str> {
    let left = || deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or("timeout");
    let addrs: Vec<_> = (url.host.as_str(), url.port).to_socket_addrs().map_err(|_| "dns")?.collect();
    if addrs.is_empty() { return Err("dns"); }
    let mut tcp = None;
    let mut last = "connect";
    for a in addrs {
        match TcpStream::connect_timeout(&a, left()?) {
            Ok(s) => { tcp = Some(s); break; }
            Err(e) => last = kind_of(&e),
        }
    }
    let tcp = tcp.ok_or(last)?;
    tcp.set_read_timeout(Some(left()?)).map_err(|_| "connect")?;
    tcp.set_write_timeout(Some(left()?)).map_err(|_| "connect")?;
    let host = if url.host.contains(':') { format!("[{}]", url.host) } else { url.host.clone() };
    let default = (url.https && url.port == 443) || (!url.https && url.port == 80);
    let host_header = if default { host } else { format!("{host}:{}", url.port) };
    let request = format!("{method} {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: FolioStudio-LinkCheck/1\r\nAccept: */*\r\nConnection: close\r\n\r\n", url.path);
    let mut stream: Box<dyn ReadWrite> = if url.https {
        let name = rustls::pki_types::ServerName::try_from(url.host.clone()).map_err(|_| "invalid")?;
        let conn = rustls::ClientConnection::new(tls(), name).map_err(|_| "tls")?;
        Box::new(rustls::StreamOwned::new(conn, tcp))
    } else { Box::new(tcp) };
    stream.write_all(request.as_bytes()).map_err(|e| kind_of(&e))?;
    let mut head = Vec::new();
    let mut buf = [0u8; 4096];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < HEADER_LIMIT {
        let n = match stream.read(&mut buf) { Ok(n) => n, Err(e) => return Err(kind_of(&e)) };
        if n == 0 { break; }
        head.extend_from_slice(&buf[..n]);
    }
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.split("\r\n");
    let status = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse::<u16>().ok()).ok_or("connect")?;
    let location = lines.take_while(|l| !l.is_empty()).find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("location")).map(|(_, v)| v.trim().to_owned()));
    Ok((status, location))
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn kind_of_status(status: u16) -> &'static str {
    match status {
        200..=399 => "ok",
        404 | 410 => "notFound",
        400..=499 => "clientError",
        _ => "serverError",
    }
}

/// One address checked: `{url, kind, status?, final?, redirects, ms}`.
fn check(url: &str, timeout: Duration) -> Value {
    let started = Instant::now();
    let deadline = started + timeout;
    let mut current = url.trim().to_owned();
    let mut redirects = 0;
    let answer = |kind: &str, status: Option<u16>, last: &str, redirects: usize| json!({
        "url": url, "kind": kind, "status": status, "final": last, "redirects": redirects, "ms": started.elapsed().as_millis() as u64,
    });
    loop {
        let parsed = match parse(&current) { Ok(p) => p, Err(kind) => return answer(kind, None, &current, redirects) };
        let mut reply = ask(&parsed, "HEAD", deadline);
        // a server that does not take HEAD: the same with GET, headers only
        if matches!(reply, Ok((405 | 501, _))) { reply = ask(&parsed, "GET", deadline); }
        match reply {
            Err(kind) => return answer(kind, None, &current, redirects),
            Ok((status @ (301 | 302 | 303 | 307 | 308), Some(location))) => {
                if redirects == MAX_REDIRECTS { return answer("tooManyRedirects", Some(status), &current, redirects); }
                redirects += 1;
                current = resolve(&parsed, &location);
            }
            Ok((status, _)) => return answer(kind_of_status(status), Some(status), &current, redirects),
        }
    }
}

/// `hyperlinkUrlStatus {urls: [...], timeoutMs?}` (uses the network): each address checked, in the order given (duplicates once).
pub fn run(req: &Value) -> Result<Value, String> {
    let mut urls: Vec<String> = Vec::new();
    for u in req["urls"].as_array().ok_or("urls (a list of web addresses) required")? {
        let u = u.as_str().ok_or("every url is text")?.trim().to_owned();
        if !u.is_empty() && !urls.contains(&u) { urls.push(u); }
    }
    if urls.len() > MAX_URLS { return Err(format!("at most {MAX_URLS} addresses at once")); }
    let timeout = Duration::from_millis(req["timeoutMs"].as_u64().unwrap_or(8000).clamp(500, 30000));
    let mut results: Vec<Value> = Vec::with_capacity(urls.len());
    for chunk in urls.chunks(PARALLEL) {
        let answers: Vec<Value> = std::thread::scope(|scope| {
            let jobs: Vec<_> = chunk.iter().map(|u| scope.spawn(move || check(u, timeout))).collect();
            jobs.into_iter().zip(chunk).map(|(j, u)| j.join().unwrap_or_else(|_| json!({"url": u, "kind": "connect", "status": null, "final": u, "redirects": 0, "ms": 0}))).collect()
        });
        results.extend(answers);
    }
    Ok(json!({"type": "hyperlinkUrlStatus", "results": results}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_parsed_and_redirections_resolved() {
        assert_eq!(parse("https://example.com/a/b?x=1#top").unwrap(), Url { https: true, host: "example.com".into(), port: 443, path: "/a/b?x=1".into() });
        assert_eq!(parse("http://h:8080").unwrap(), Url { https: false, host: "h".into(), port: 8080, path: "/".into() });
        assert_eq!(parse("http://[::1]:81/x").unwrap().host, "::1");
        assert_eq!(parse("mailto:a@b.c"), Err("notChecked"));
        assert_eq!(parse("no address"), Err("invalid"));
        let base = parse("https://example.com/docs/page.html").unwrap();
        assert_eq!(resolve(&base, "/new"), "https://example.com/new");
        assert_eq!(resolve(&base, "other.html"), "https://example.com/docs/other.html");
        assert_eq!(resolve(&base, "//cdn.example.com/x"), "https://cdn.example.com/x");
        assert_eq!(resolve(&base, "http://elsewhere/"), "http://elsewhere/");
    }

    /// The real network (run by hand: `cargo test -p folio-engine --bin folio-engine -- urlcheck --ignored`): HTTPS with the root list.
    #[test]
    #[ignore]
    fn https_on_the_network() {
        let out = run(&json!({"urls": ["https://example.com/", "https://www.rust-lang.org/no-such-page-folio", "https://no-such-host.invalid/"]})).unwrap();
        println!("{out:#}");
        let r = out["results"].as_array().unwrap();
        assert_eq!(r[0]["kind"], "ok");
        assert_eq!(r[2]["kind"], "dns");
    }

    /// A local server (no outside network): a redirection followed to 200, a 404, a refused HEAD answered by GET, nothing listening.
    #[test]
    fn statuses_from_a_local_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(5) {
                let mut s = stream.unwrap();
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or("").to_owned();
                let reply = if line.starts_with("HEAD /old ") { "HTTP/1.1 301 Moved\r\nLocation: /new\r\nContent-Length: 0\r\n\r\n" }
                    else if line.starts_with("HEAD /new ") { "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n" }
                    else if line.starts_with("HEAD /nohead ") { "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n" }
                    else if line.starts_with("GET /nohead ") { "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok" }
                    else { "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n" };
                let _ = s.write_all(reply.as_bytes());
            }
        });
        let base = format!("http://127.0.0.1:{port}");
        let closed = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
        let out = run(&json!({"urls": [format!("{base}/old"), format!("{base}/missing"), format!("{base}/nohead"), format!("http://127.0.0.1:{closed}/"), "mailto:x@y.z"], "timeoutMs": 3000})).unwrap();
        let r = out["results"].as_array().unwrap();
        let get = |u: &str| r.iter().find(|x| x["url"] == u).unwrap().clone();
        let old = get(&format!("{base}/old"));
        assert_eq!((old["kind"].as_str(), old["status"].as_u64(), old["redirects"].as_u64()), (Some("ok"), Some(200), Some(1)), "{old}");
        assert_eq!(old["final"], format!("{base}/new"));
        assert_eq!(get(&format!("{base}/missing"))["kind"], "notFound");
        assert_eq!(get(&format!("{base}/nohead"))["status"], 200);
        assert_eq!(get(&format!("http://127.0.0.1:{closed}/"))["kind"], "connect");
        assert_eq!(get("mailto:x@y.z")["kind"], "notChecked");
    }
}
