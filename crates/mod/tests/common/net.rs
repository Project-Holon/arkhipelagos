//! The tests' side of a metaxy link, framed as eidos's port frames it
//! (SPEC section 3): connections paced under metaxy's limit, frames, `F`
//! pieces put back together.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub const V2: &str = "eidos-net/2";

/// metaxy takes at most 10 new connections per address per minute and
/// refuses more with a reset (SPEC section 6); the test keeps under it.
/// The times are kept in a file in the temp folder, so runs one after the
/// other share them.
pub fn pace() {
    let f = std::env::temp_dir().join("arkh-metaxy-connections.txt");
    let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    let mut times: Vec<f64> = std::fs::read_to_string(&f)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.parse().ok())
        .filter(|t| now() - t < 61.0)
        .collect();
    if times.len() >= 9 {
        let wait = 61.0 - (now() - times[0]);
        if wait > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(wait));
        }
        times.remove(0);
    }
    times.push(now());
    let text: Vec<String> = times.iter().map(|t| t.to_string()).collect();
    std::fs::write(&f, text.join("\n")).ok();
}

pub fn trace() -> bool {
    std::env::var("ARKH_NET_TRACE").is_ok()
}

pub struct Link {
    s: TcpStream,
    buf: Vec<u8>,
    pieces: BTreeMap<String, Vec<u8>>,
    pub closed: bool,
}

impl Link {
    pub fn open(addr: &str) -> Link {
        pace();
        let s = TcpStream::connect(addr).expect("metaxy");
        s.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        Link { s, buf: Vec::new(), pieces: BTreeMap::new(), closed: false }
    }
    /// false when metaxy closed the connection
    pub fn send(&mut self, t: char, body: &[u8]) -> bool {
        let mut f = format!("{t} {}\n", body.len()).into_bytes();
        f.extend_from_slice(body);
        self.s.write_all(&f).is_ok()
    }
    /// The next whole frame, big payloads put back together; None after
    /// `wait` with nothing.
    pub fn next(&mut self, wait: Duration) -> Option<(char, Vec<u8>)> {
        let end = Instant::now() + wait;
        loop {
            if let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
                let head = String::from_utf8_lossy(&self.buf[..nl]).to_string();
                let (t, len) = head.split_once(' ').unwrap();
                let len: usize = len.parse().unwrap();
                if self.buf.len() >= nl + 1 + len {
                    let body = self.buf[nl + 1..nl + 1 + len].to_vec();
                    self.buf.drain(..nl + 1 + len);
                    let t = t.chars().next().unwrap();
                    if t == 'F' {
                        let p = body.iter().position(|&b| b == b'\n').unwrap();
                        let h = String::from_utf8_lossy(&body[..p]).to_string();
                        let id = h.split(' ').next().unwrap().to_string();
                        self.pieces.entry(id).or_default().extend_from_slice(&body[p + 1..]);
                        continue;
                    }
                    // `@<id>` in place of the body (after the first line for S)
                    let text = String::from_utf8_lossy(&body).to_string();
                    let (first, rest) = text.split_once('\n').map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((text.clone(), None));
                    let at = rest.as_deref().unwrap_or(&first).to_string();
                    if let Some(id) = at.strip_prefix('@') {
                        if let Some(whole) = self.pieces.remove(id) {
                            let body = match rest {
                                Some(_) => [format!("{first}\n").into_bytes(), whole].concat(),
                                None => whole,
                            };
                            if trace() {
                                eprintln!("<- {t} (pieces) {}", String::from_utf8_lossy(&body[..40.min(body.len())]));
                            }
                            return Some((t, body));
                        }
                    }
                    if trace() {
                        eprintln!("<- {t} {}", String::from_utf8_lossy(&body[..body.len().min(60)]));
                    }
                    return Some((t, body));
                }
            }
            if Instant::now() > end {
                return None;
            }
            let mut b = [0u8; 65536];
            match self.s.read(&mut b) {
                Ok(0) => {
                    self.closed = true;
                    return None;
                }
                Ok(n) => self.buf.extend_from_slice(&b[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => {}
                Err(e) => {
                    if trace() {
                        eprintln!("<- (link lost: {e})");
                    }
                    self.closed = true;
                    return None;
                }
            }
        }
    }
}

pub fn token() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("{:032x}", t ^ 0x5a5a_1234_9876_abcd_u128)
}
