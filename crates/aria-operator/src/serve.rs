//! Hosted shell: `work --serve ADDR`. One static binary hosts the whole
//! catalog over HTTP/1.1 with the standard library only (minimal-dependency
//! doctrine; wasm32 untouched since this module is `cfg(feature = "cli")`).
//!
//! Routes (all JSON, `Connection: close`):
//!   GET  /health    → {"ok":true,"catalog":560,"version":…,"pool":…}
//!   GET  /commands  → aria-work-commands-v1 (what Aria compiles against)
//!   GET  /dispatch  → aria-dispatch-v1 (registry descriptor)
//!   POST /work      → body: work-v1 command {work|ops, in}  → callback
//!   POST /harness   → body: pcvc-aria-telemetry-request-v1  → bound result
//!
//! Scale discipline (a fleet of workers, not a browser):
//!   - fixed worker pool + bounded queue: a flood never spawns unbounded
//!     threads; past the queue the node answers `503 Retry-After` instantly
//!     instead of queueing latency (backpressure beats lag);
//!   - socket read/write timeouts: a stalled client cannot pin a worker;
//!   - static routes are serialized once per process and served as bytes;
//!   - no shared mutable state anywhere (the node is stateless), so there is
//!     nothing to lock and nothing to deadlock — the only sync primitive is
//!     the queue itself.

use crate::{commands_json, dispatch_json, execute_work, harness_lane, RunOpts, WorkRequest};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Largest request body accepted.
const MAX_BODY: usize = 8 * 1024 * 1024;
/// Per-socket read/write deadline. A harness that waits longer has a
/// deadline of its own; the node must not hold a worker for a dead peer.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
/// Queued connections beyond the pool before the node sheds load (503).
const QUEUE_DEPTH: usize = 1024;

/// Worker threads: 4× cores. Requests are CPU-bound but short; the extra
/// headroom keeps cores busy while a few sockets drain.
fn pool_size() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get() * 4)
}

/// Bind and serve forever. Returns only on bind failure.
pub fn serve(addr: &str, opts: &RunOpts) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    let workers = pool_size();
    // Startup line to stdout (not stderr — harness lanes keep stderr silent).
    println!(
        "{}",
        serde_json::json!({
            "serving": listener.local_addr()?.to_string(),
            "catalog": crate::catalog().len(),
            "pool": workers,
            "queue": QUEUE_DEPTH,
        })
    );
    let (tx, rx) = sync_channel::<TcpStream>(QUEUE_DEPTH);
    spawn_pool(workers, rx, opts);
    accept_loop(&listener, &tx);
    Ok(())
}

/// Same as [`serve`] but returns the bound address and runs the accept loop
/// on a background thread — the seam the load test uses.
pub fn serve_background(addr: &str, opts: &RunOpts) -> io::Result<std::net::SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let local = listener.local_addr()?;
    let (tx, rx) = sync_channel::<TcpStream>(QUEUE_DEPTH);
    spawn_pool(pool_size(), rx, opts);
    std::thread::spawn(move || accept_loop(&listener, &tx));
    Ok(local)
}

fn spawn_pool(workers: usize, rx: Receiver<TcpStream>, opts: &RunOpts) {
    let rx = Arc::new(Mutex::new(rx));
    for _ in 0..workers {
        let rx = Arc::clone(&rx);
        let opts = opts.clone();
        std::thread::spawn(move || loop {
            // Lock only to dequeue; the request itself runs unlocked.
            let next = match rx.lock() {
                Ok(r) => r.recv(),
                Err(_) => return,
            };
            match next {
                Ok(stream) => {
                    let _ = handle(stream, &opts);
                }
                Err(_) => return,
            }
        });
    }
}

fn accept_loop(listener: &TcpListener, tx: &SyncSender<TcpStream>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
        let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));
        let _ = stream.set_nodelay(true);
        match tx.try_send(stream) {
            Ok(()) => {}
            Err(TrySendError::Full(mut s)) => {
                // Shed load immediately: cheaper for the fleet than waiting.
                let _ = respond(&mut s, 503, br#"{"error":"node saturated; retry"}"#);
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

/// Static routes are a function of the process, not the request.
fn static_bytes(route: &str) -> &'static [u8] {
    static HEALTH: OnceLock<Vec<u8>> = OnceLock::new();
    static COMMANDS: OnceLock<Vec<u8>> = OnceLock::new();
    static DISPATCH: OnceLock<Vec<u8>> = OnceLock::new();
    match route {
        "/health" => HEALTH.get_or_init(|| {
            serde_json::to_vec(&serde_json::json!({
                "ok": true,
                "catalog": crate::catalog().len(),
                "version": env!("CARGO_PKG_VERSION"),
                "stateless": true,
                "pool": pool_size(),
                "queue": QUEUE_DEPTH,
            }))
            .unwrap_or_default()
        }),
        "/commands" => COMMANDS.get_or_init(|| serde_json::to_vec(&commands_json()).unwrap_or_default()),
        _ => DISPATCH.get_or_init(|| serde_json::to_vec(&dispatch_json()).unwrap_or_default()),
    }
}

fn handle(mut stream: TcpStream, opts: &RunOpts) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    if len > MAX_BODY {
        return respond(&mut stream, 413, br#"{"error":"body too large"}"#);
    }
    let mut body = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut body)?;
    }
    let route = path.split('?').next().unwrap_or(path);
    let (code, out): (u16, Vec<u8>) = match (method, route) {
        ("GET", "/health" | "/commands" | "/dispatch") => {
            return respond(&mut stream, 200, static_bytes(route));
        }
        ("POST", "/work") => match serde_json::from_slice::<WorkRequest>(&body) {
            Ok(req) => match execute_work(&req, opts) {
                Ok(v) => (200, serde_json::to_vec(&v).unwrap_or_default()),
                Err(e) => (422, err_json(&e.to_string())),
            },
            Err(e) => (400, err_json(&e.to_string())),
        },
        ("POST", "/harness") => {
            let (exit, bytes) = harness_lane(&body);
            (if exit == 0 { 200 } else { 400 }, bytes)
        }
        _ => (404, err_json("unknown route")),
    };
    respond(&mut stream, code, &out)
}

fn err_json(msg: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"error": msg})).unwrap_or_default()
}

fn respond(stream: &mut TcpStream, code: u16, body: &[u8]) -> io::Result<()> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let retry = if code == 503 { "Retry-After: 1\r\n" } else { "" };
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}
