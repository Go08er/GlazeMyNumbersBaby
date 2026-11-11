//! The currency fetch against a server of the test's own on the loopback
//! interface (no real host): the client's whole HTTP path, with the TLS
//! stack's plain-HTTP side, bad bodies, and a stalled peer.
#![cfg(feature = "network")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use unitconv::currency::{FetchConfig, fetch_latest_with};

const V2_RATES: &str = include_str!("fixtures/frankfurter_v2_rates.json");
const V2_CURRENCIES: &str = include_str!("fixtures/frankfurter_v2_currencies.json");
const V1_LATEST: &str = include_str!("fixtures/frankfurter_v1_latest.json");
const V1_CURRENCIES: &str = include_str!("fixtures/frankfurter_v1_currencies.json");

/// Serves `handler(path)` -> (status, body) to every connection until the
/// test ends (the thread is left to the process).
fn serve(handler: fn(&str) -> (u16, String)) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    thread::spawn(move || {
        for s in l.incoming().flatten() {
            thread::spawn(move || answer(s, handler));
        }
    });
    format!("http://{addr}")
}

fn answer(mut s: TcpStream, handler: fn(&str) -> (u16, String)) {
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut buf = Vec::new();
    let mut b = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        match s.read(&mut b) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&b[..n]),
        }
    }
    let req = String::from_utf8_lossy(&buf);
    let path = req.split_whitespace().nth(1).unwrap_or("").to_owned();
    let (status, body) = handler(&path);
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body.as_bytes());
}

fn cfg(root: String) -> FetchConfig {
    FetchConfig {
        api_root: root,
        timeout: Duration::from_secs(3),
        ..Default::default()
    }
}

#[test]
fn v2_over_loopback() {
    let root = serve(|p| {
        if p.starts_with("/v2/rates") {
            (200, V2_RATES.to_owned())
        } else if p == "/v2/currencies" {
            (200, V2_CURRENCIES.to_owned())
        } else {
            (404, String::new())
        }
    });
    let s = fetch_latest_with(&cfg(root)).expect("v2");
    assert_eq!(s.base, "USD");
    assert!(s.rate("EUR").is_some_and(|r| r.rate > 0.0));
}

#[test]
fn v1_fallback_when_v2_is_missing() {
    let root = serve(|p| {
        if p.starts_with("/v1/latest") {
            (200, V1_LATEST.to_owned())
        } else if p == "/v1/currencies" {
            (200, V1_CURRENCIES.to_owned())
        } else {
            (404, String::new())
        }
    });
    let s = fetch_latest_with(&cfg(root)).expect("v1 fallback");
    assert!(s.rate("EUR").is_some());
}

#[test]
fn errors_and_garbage_are_errors_not_hangs() {
    let t = Instant::now();
    let root = serve(|_| (500, "no".into()));
    assert!(fetch_latest_with(&cfg(root)).is_err());
    let root = serve(|_| (200, "not json".into()));
    assert!(fetch_latest_with(&cfg(root)).is_err());
    let root = serve(|_| (200, "x".repeat(9 * 1024 * 1024)));
    assert!(fetch_latest_with(&cfg(root)).is_err(), "over the 8 MiB cap");
    assert!(t.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_stalled_server_times_out() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    thread::spawn(move || {
        let mut held = Vec::new();
        for s in l.incoming().flatten() {
            held.push(s); // accept, never answer
        }
    });
    let t = Instant::now();
    let mut c = cfg(format!("http://{addr}"));
    c.timeout = Duration::from_secs(1);
    assert!(fetch_latest_with(&c).is_err());
    assert!(t.elapsed() < Duration::from_secs(8), "{:?}", t.elapsed());
}
