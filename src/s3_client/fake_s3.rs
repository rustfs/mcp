// Copyright 2024 RustFS Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! An in-process S3 endpoint for tests. It answers scripted responses and
//! records what the client sent, so that the "not configured" branches of
//! the data layer run against the real SDK error parsing instead of against
//! hand-built error values: which error code a backend answers is exactly
//! what those branches decide on.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the endpoint waits for one request or its answer to write
/// before giving up, so that a broken exchange fails a test instead of
/// hanging it.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);

/// One request as the fake endpoint received it.
#[derive(Debug, Clone)]
pub(crate) struct RecordedRequest {
    /// `GET`, `HEAD`, `PUT`, or `DELETE`.
    pub method: String,
    /// Bucket of the path-style request, e.g. `locked` for `/locked/doc`.
    pub bucket: String,
    /// Key of the request; empty when it addresses the bucket itself.
    pub key: String,
    query: Vec<(String, String)>,
    /// Headers by lower-case name.
    headers: Vec<(String, String)>,
    /// Request body.
    pub body: Vec<u8>,
}

impl RecordedRequest {
    /// Whether the request carries the given subresource, e.g. `retention`.
    pub(crate) fn has(&self, subresource: &str) -> bool {
        self.query.iter().any(|(name, _)| name == subresource)
    }

    /// Decoded value of a query parameter.
    pub(crate) fn query(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Value of a header, looked up by its lower-case name.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The request body as text, for assertions on what was sent.
    pub(crate) fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// One scripted answer of the fake endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Answer {
    status: u16,
    body: String,
}

impl Answer {
    /// `200` with the given XML body.
    pub(crate) fn ok(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: body.into(),
        }
    }

    /// An S3 error document, the shape every backend gives its error codes.
    pub(crate) fn error(status: u16, code: &str, message: &str) -> Self {
        Self {
            status,
            body: format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
                 <Error><Code>{code}</Code><Message>{message}</Message>\
                 <RequestId>request-id</RequestId><HostId>host-id</HostId></Error>"
            ),
        }
    }
}

/// An empty `200`, what S3 answers to most writes and to a bucket HEAD.
pub(crate) const OK: Answer = Answer {
    status: 200,
    body: String::new(),
};

/// The fake endpoint: live from `start` until dropped.
#[derive(Debug)]
pub(crate) struct FakeS3 {
    endpoint: String,
    address: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
}

impl FakeS3 {
    /// Start the endpoint. `answer` decides the response to each request.
    pub(crate) fn start(
        answer: impl Fn(&RecordedRequest) -> Answer + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a loopback port is available");
        let address = listener
            .local_addr()
            .expect("the listener has a local address")
            .to_string();
        let endpoint = format!("http://{address}");

        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let answer: Arc<dyn Fn(&RecordedRequest) -> Answer + Send + Sync> = Arc::new(answer);

        {
            let (requests, stop) = (Arc::clone(&requests), Arc::clone(&stop));
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    // `Drop` wakes the blocked `accept`; nothing follows it.
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    // A failed exchange surfaces as the client error it
                    // causes, which fails the assertion waiting on it.
                    if let Ok(stream) = stream {
                        let _ = serve(stream, answer.as_ref(), &requests);
                    }
                }
            });
        }

        Self {
            endpoint,
            address,
            requests,
            stop,
        }
    }

    /// Where a client reaches the endpoint, e.g. `http://127.0.0.1:40000`.
    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Everything received so far, in order.
    pub(crate) fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("poisoned").clone()
    }
}

impl Drop for FakeS3 {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblock the accepting thread; it exits without answering.
        let _ = TcpStream::connect(self.address.as_str());
    }
}

/// Read one request, answer it, and record it.
fn serve(
    mut stream: TcpStream,
    answer: &(dyn Fn(&RecordedRequest) -> Answer + Send + Sync),
    requests: &Mutex<Vec<RecordedRequest>>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(EXCHANGE_TIMEOUT))?;
    stream.set_write_timeout(Some(EXCHANGE_TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();

    let mut content_length = 0usize;
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
            headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;

    let (bucket, key, query) = parse_target(&target);
    let request = RecordedRequest {
        method,
        bucket,
        key,
        query,
        headers,
        body,
    };
    requests.lock().expect("poisoned").push(request.clone());

    let response = answer(&request);
    // A HEAD carries headers only, whatever the answer's body says.
    let body = if request.method == "HEAD" {
        ""
    } else {
        response.body.as_str()
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\n\
         Content-Type: application/xml\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{}",
        response.status,
        reason(response.status),
        body.len(),
        body
    )?;
    stream.flush()?;

    Ok(())
}

/// The reason phrase of the statuses the tests script.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        _ => "Status",
    }
}

/// Split a request target into its bucket, key, and decoded query.
fn parse_target(target: &str) -> (String, String, Vec<(String, String)>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut segments = path.trim_start_matches('/').splitn(2, '/');
    let bucket = percent_decode(segments.next().unwrap_or_default(), false);
    let key = segments
        .next()
        .map(|key| percent_decode(key, false))
        .unwrap_or_default();
    let query = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(name, true), percent_decode(value, true))
        })
        .collect();
    (bucket, key, query)
}

/// Decode percent escapes; in query strings a `+` is a space as well.
fn percent_decode(value: &str, plus_as_space: bool) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 3 <= bytes.len() => {
                match u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    Ok(byte) => decoded.push(byte),
                    Err(_) => decoded.push(b'%'),
                }
                index += 3;
            }
            b'+' if plus_as_space => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_splits_bucket_key_and_query() {
        // A subresource arrives as a flag with an empty value.
        let (bucket, key, query) = parse_target("/locked/doc.txt?retention=&versionId=v%201");
        assert_eq!(bucket, "locked");
        assert_eq!(key, "doc.txt");
        assert_eq!(
            query,
            vec![
                ("retention".to_string(), String::new()),
                ("versionId".to_string(), "v 1".to_string())
            ]
        );

        // A request to the bucket itself has no key.
        let (bucket, key, query) = parse_target("/locked?object-lock=");
        assert_eq!(bucket, "locked");
        assert!(key.is_empty());
        assert_eq!(query.len(), 1);
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("a%2Fb%3Fc", false), "a/b?c");
        assert_eq!(percent_decode("a+b", true), "a b");
        assert_eq!(percent_decode("a+b", false), "a+b");
        // An incomplete escape and multibyte text survive.
        assert_eq!(percent_decode("100%", false), "100%");
        assert_eq!(percent_decode("%E6%96%87", false), "文");
    }

    #[test]
    fn test_answer_error_is_an_s3_error_document() {
        let answer = Answer::error(404, "NoSuchKey", "The specified key does not exist.");
        assert_eq!(answer.status, 404);
        assert!(answer.body.contains("<Code>NoSuchKey</Code>"));
        assert!(
            answer
                .body
                .contains("<Message>The specified key does not exist.</Message>")
        );
        assert_eq!(OK.status, 200);
    }
}
