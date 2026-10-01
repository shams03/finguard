//! Minimal scripted HTTP/1.1 server for client tests (no external mock crate).
#![allow(dead_code)]
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[derive(Clone)]
pub struct Scripted {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub delay_ms: u64,
}

impl Scripted {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.into(),
            delay_ms: 0,
        }
    }
    pub fn with_header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub fn delayed(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }
}

pub struct TestServer {
    pub url: String,
    pub requests: Arc<Mutex<Vec<String>>>,
}

impl TestServer {
    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

/// Serve `script` in order; the last entry repeats forever.
pub async fn serve(script: Vec<Scripted>) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let reqs = requests.clone();
    tokio::spawn(async move {
        let mut i = 0usize;
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let item = script[i.min(script.len() - 1)].clone();
            i += 1;
            let reqs = reqs.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = find(&buf, b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if buf.len() >= pos + 4 + len {
                            break;
                        }
                    }
                }
                reqs.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).into_owned());
                if item.delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(item.delay_ms)).await;
                }
                let mut resp = format!(
                    "HTTP/1.1 {} X\r\ncontent-length: {}\r\nconnection: close\r\n",
                    item.status,
                    item.body.len()
                );
                for (k, v) in &item.headers {
                    resp.push_str(&format!("{k}: {v}\r\n"));
                }
                resp.push_str("\r\n");
                resp.push_str(&item.body);
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    TestServer { url, requests }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}
