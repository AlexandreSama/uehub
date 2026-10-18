//! Lecture de la page unrealengine.com/linux.
//!
//! Le serveur écrit la liste des zip directement dans la page, en JSON rangé dans un
//! script : { "name": "Linux_Unreal_Engine_5.8.3.zip", "createdAt": …, "size": …,
//! "downloadUrl": "https://…" }. Ce JSON est échappé (\" au lieu de ") : on le décode
//! avant de le lire.

use serde::Deserialize;

use super::{EngineDownload, engine_version, version_key};

/// Un zip tel que la page le décrit. Les autres champs sont ignorés.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageBlob {
    name: String,
    size: u64,
    created_at: Option<String>,
    download_url: String,
}

/// Les zip du moteur trouvés dans la page, du plus récent au plus ancien.
pub(super) fn parse_linux_page(html: &str) -> Vec<EngineDownload> {
    let text = unescape(html);
    let mut found: Vec<EngineDownload> = Vec::new();
    // Chaque nom de zip entre guillemets marque un objet à lire autour de lui.
    for (at, _) in text.match_indices(r#""Linux_Unreal_Engine_"#) {
        let Some(blob) = blob_around(&text, at) else {
            continue;
        };
        let Some(version) = engine_version(&blob.name) else {
            continue;
        };
        if found.iter().any(|d| d.file_name == blob.name) {
            continue;
        }
        found.push(EngineDownload {
            version,
            file_name: blob.name,
            size: blob.size,
            created_at: blob.created_at,
            url: blob.download_url,
        });
    }
    found.sort_by_key(|d| std::cmp::Reverse(version_key(&d.version)));
    found
}

/// L'objet JSON qui contient ce nom. On essaie les quelques `{` précédents, au cas
/// où `name` ne serait pas le premier champ.
fn blob_around(text: &str, at: usize) -> Option<PageBlob> {
    let mut end = at;
    for _ in 0..5 {
        let open = text[..end].rfind('{')?;
        let mut values = serde_json::Deserializer::from_str(&text[open..]).into_iter::<PageBlob>();
        if let Some(Ok(blob)) = values.next()
            && blob.name.starts_with("Linux_Unreal_Engine_")
        {
            return Some(blob);
        }
        end = open;
    }
    None
}

/// Défait l'échappement du JSON rangé dans une chaîne JavaScript, autant de fois qu'il
/// le faut (une chaîne dans une chaîne est échappée deux fois), et les &amp; du HTML.
fn unescape(html: &str) -> String {
    let mut text = html.replace("&amp;", "&");
    for _ in 0..4 {
        let next = decode_pass(&text);
        if next == text {
            break;
        }
        text = next;
    }
    text
}

/// Un niveau d'échappement : `\"` devient `"`, `\\` devient `\`, `\/` devient `/`, et
/// `\u0026` devient `&`. Les autres séquences (`\n`…), et les `\u` qui donneraient un
/// guillemet ou une barre oblique inverse, restent telles quelles pour ne pas casser le JSON.
fn decode_pass(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some(next @ ('"' | '\\' | '/')) => {
                out.push(next);
                chars.next();
            }
            Some('u') => {
                let hex: String = chars.clone().skip(1).take(4).collect();
                let decoded = u32::from_str_radix(&hex, 16)
                    .ok()
                    .filter(|_| hex.len() == 4)
                    .and_then(char::from_u32)
                    .filter(|c| !c.is_control() && *c != '"' && *c != '\\');
                match decoded {
                    Some(decoded) => {
                        out.push(decoded);
                        for _ in 0..5 {
                            chars.next();
                        }
                    }
                    None => out.push('\\'),
                }
            }
            _ => out.push('\\'),
        }
    }
    out
}