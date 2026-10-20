//! Pour les tests du core : un faux serveur HTTP sur 127.0.0.1. Les tests lui parlent à
//! la place d'Epic, de Fab ou du CDN : ils marchent hors ligne, et ne touchent à aucun
//! compte.

use std::collections::HashMap;
use std::sync::Arc;

use reqwest::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Une requête reçue par le faux serveur.
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    /// Un en-tête, quelle que soit sa casse.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Le client a-t-il envoyé ce cookie ?
    pub fn has_cookie(&self, name: &str) -> bool {
        let prefix = format!("{name}=");
        self.header("cookie")
            .is_some_and(|cookies| cookies.split(';').any(|c| c.trim().starts_with(&prefix)))
    }
}

/// Lit une requête entière : les en-têtes, puis le corps annoncé par Content-Length.
pub async fn read_request(sock: &mut TcpStream) -> Option<Request> {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    let head_end = loop {
        let n = sock.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break end;
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let url = Url::parse(&format!("http://local{}", first.next()?)).ok()?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    let length: usize = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    while data.len() < head_end + 4 + length {
        let n = sock.read(&mut buf).await.ok()?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    Some(Request {
        method,
        path: url.path().to_string(),
        query: url.query_pairs().into_owned().collect(),
        headers,
        body: String::from_utf8_lossy(&data[head_end + 4..]).into_owned(),
    })
}

/// Une réponse du faux serveur.
pub struct Reply {
    status: String,
    headers: String,
    body: Vec<u8>,
}

impl Reply {
    /// "200 OK", "404 Not Found"…
    pub fn new(status: &str) -> Self {
        Self {
            status: status.into(),
            headers: String::new(),
            body: Vec::new(),
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push_str(&format!("{name}: {value}\r\n"));
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

/// Écrit la réponse ; la connexion se ferme ensuite.
pub async fn respond(sock: &mut TcpStream, reply: Reply) {
    let head = format!(
        "HTTP/1.1 {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.headers,
        reply.body.len()
    );
    let _ = sock.write_all(head.as_bytes()).await;
    let _ = sock.write_all(&reply.body).await;
}

/// Un faux serveur : chaque requête passe par `answer`, qui reçoit aussi l'adresse du
/// serveur, pour les redirections. Renvoie cette adresse, ex. "http://127.0.0.1:41234".
/// Le serveur s'arrête avec le test.
pub async fn serve<F>(answer: F) -> String
where
    F: Fn(Request, &str) -> Reply + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let answer = Arc::new(answer);
    let address = base.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let (answer, address) = (answer.clone(), address.clone());
            tokio::spawn(async move {
                if let Some(request) = read_request(&mut sock).await {
                    respond(&mut sock, answer(request, &address)).await;
                }
            });
        }
    });
    base
}

/// Une adresse où rien n'écoute : la connexion y est refusée tout de suite.
pub fn dead_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    address
}
