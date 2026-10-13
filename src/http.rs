use std::time::{Duration, Instant};
use wasi::{
    http::{outgoing_handler, types::*},
    io::streams::{InputStream, StreamError},
};

// Poll at most one bounded chunk per tick; never wait on network I/O in a host callback.
pub struct Request {
    ready: Option<wasi::io::poll::Pollable>,
    stream: Option<InputStream>,
    body: Option<IncomingBody>,
    response: Option<IncomingResponse>,
    future: Option<FutureIncomingResponse>,
    bytes: Vec<u8>,
    start: Instant,
}
impl Request {
    pub fn start(host: &str, path: &str) -> Result<Self, String> {
        let fields = Fields::new();
        fields
            .append("user-agent", b"Rust-CustomSkins/2.0.0")
            .map_err(|e| format!("{e:?}"))?;
        let request = OutgoingRequest::new(fields);
        request
            .set_scheme(Some(&Scheme::Https))
            .map_err(|_| "Invalid scheme")?;
        request
            .set_authority(Some(host))
            .map_err(|_| "Invalid host")?;
        request
            .set_path_with_query(Some(path))
            .map_err(|_| "Invalid path")?;
        let options = RequestOptions::new();
        options
            .set_connect_timeout(Some(3_000_000_000))
            .map_err(|_| "Invalid timeout")?;
        options
            .set_first_byte_timeout(Some(5_000_000_000))
            .map_err(|_| "Invalid timeout")?;
        options
            .set_between_bytes_timeout(Some(3_000_000_000))
            .map_err(|_| "Invalid timeout")?;
        let future =
            outgoing_handler::handle(request, Some(options)).map_err(|e| format!("{e:?}"))?;
        Ok(Self {
            ready: Some(future.subscribe()),
            stream: None,
            body: None,
            response: None,
            future: Some(future),
            bytes: vec![],
            start: Instant::now(),
        })
    }
    pub fn poll(&mut self) -> Option<Result<Option<Vec<u8>>, String>> {
        if self.start.elapsed() > Duration::from_secs(8) {
            return Some(Err("Official API timed out".into()));
        }
        if let Some(future) = &self.future {
            if !self.ready.as_ref()?.ready() {
                return None;
            }
            let result = future.get()?;
            let response = match result {
                Ok(Ok(r)) => r,
                other => return Some(Err(format!("Official API failed: {other:?}"))),
            };
            self.ready = None;
            self.future = None;
            if matches!(response.status(), 204 | 404) {
                return Some(Ok(None));
            }
            if response.status() != 200 {
                return Some(Err(format!(
                    "Official API returned HTTP {}",
                    response.status()
                )));
            }
            let body = match response.consume() {
                Ok(b) => b,
                Err(_) => return Some(Err("Missing response body".into())),
            };
            self.stream = match body.stream() {
                Ok(s) => Some(s),
                Err(_) => return Some(Err("Unreadable response body".into())),
            };
            self.body = Some(body);
            self.response = Some(response);
        }
        if !self.stream.as_ref()?.subscribe().ready() {
            return None;
        }
        match self.stream.as_ref()?.read(8192) {
            Ok(bytes) => {
                self.bytes.extend_from_slice(&bytes);
                if self.bytes.len() > 65536 {
                    return Some(Err("Official response too large".into()));
                }
                None
            }
            Err(StreamError::Closed) => Some(Ok(Some(std::mem::take(&mut self.bytes)))),
            Err(e) => Some(Err(format!("Official response failed: {e:?}"))),
        }
    }
}
