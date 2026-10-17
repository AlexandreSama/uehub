//! Le protocole `thumb://` : l'interface y demande les vignettes,
//! Rust les sert depuis le cache disque du core.

use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, header};
use uehub_core::ThumbCache;

/// Répond à `thumb://localhost/<URL de la vignette, encodée>`.
pub async fn respond(cache: &ThumbCache, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let encoded = request.uri().path().trim_start_matches('/');
    let url = percent_decode_str(encoded).decode_utf8_lossy();

    match cache.get(&url).await {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, image_type(&bytes))
            // L'interface lit la vignette avec fetch depuis une autre origine.
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(bytes)
            .unwrap(),
        Err(e) => {
            eprintln!("vignette {url} : {e}");
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(Vec::new())
                .unwrap()
        }
    }
}

/// Le type d'image, d'après ses premiers octets.
fn image_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\xFF\xD8") {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.get(8..12) == Some(&b"WEBP"[..]) {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}