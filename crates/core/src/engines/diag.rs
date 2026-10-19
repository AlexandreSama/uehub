//! Diagnostic de la session unrealengine.com (essai, pas pour l'app).
//!
//! Ouvre une session sur unrealengine.com, suit la connexion silencieuse que le site
//! demande, puis cherche par où passe la liste des moteurs. N'affiche jamais de code, de
//! jeton ni de valeur de cookie : seulement des statuts HTTP, des noms de cookies et de
//! champs, des adresses d'API publiques et les messages d'erreur d'Epic.

use std::collections::BTreeSet;
use std::sync::Arc;

use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, LOCATION, REFERER, SET_COOKIE, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Response, Url};
use serde_json::Value;

use super::{Sites, api, page};
use crate::{CoreError, EpicClient, Result};

const JSON: &str = "application/json";
const HTML: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

fn blobs_url(sites: &Sites) -> String {
    format!("{}/api/blobs/linux", sites.unreal)
}

/// Les deux parcours qu'on essaie.
pub(super) enum Variant {
    /// Le parcours d'egs-api (avec set-sid), puis la connexion silencieuse
    Full,
    /// Sans set-sid : directement la connexion silencieuse, avec l'adresse vue en A
    NoSetSid(Url),
}

/// Les en-têtes qu'egs-api envoie à chaque requête.
fn launcher_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "UELauncher/17.0.1-37584233+++Portal+Release-Live Windows/10.0.19043.1.0.64bit",
        ),
    );
    headers.insert(
        "X-Epic-Correlation-ID",
        HeaderValue::from_static(
            "UE4-c176f7154c2cda1061cc43ab52598e2b-93AFB486488A22FDF70486BD1D883628-BFCD88F649E997BA203FF69F07CE578C",
        ),
    );
    headers
}

impl EpicClient {
    /// Raconte, pour chaque parcours, ce qu'Epic répond.
    pub async fn diagnose_unreal_session(&mut self, report: &mut dyn FnMut(String)) -> Result<()> {
        self.resume().await?;
        let account_id = self.egs.user_details().account_id;
        let sites = Sites::real();

        report(String::new());
        report("=== A. Parcours d'egs-api, puis connexion silencieuse ===".into());
        let code = self.exchange_code().await?;
        let silent = run(&sites, Variant::Full, &code, account_id.as_deref(), report)
            .await
            .unwrap_or_else(|e| {
                report(format!("   erreur : {e}"));
                None
            });

        report(String::new());
        report("=== D. Sans set-sid : directement la connexion silencieuse ===".into());
        let Some(silent) = silent else {
            report("   impossible : A n'a pas reçu d'adresse de connexion silencieuse".into());
            return Ok(());
        };
        let code = self.exchange_code().await?;
        if let Err(e) = run(
            &sites,
            Variant::NoSetSid(silent),
            &code,
            account_id.as_deref(),
            report,
        )
        .await
        {
            report(format!("   erreur : {e}"));
        }
        Ok(())
    }

    /// Un code d'échange ne sert qu'une fois : un nouveau par parcours.
    async fn exchange_code(&mut self) -> Result<String> {
        Ok(self
            .egs
            .try_game_token()
            .await
            .map_err(api("code d'échange"))?
            .code)
    }
}

/// Un parcours complet. Renvoie l'adresse de connexion silencieuse qu'il a reçue.
pub(super) async fn run(
    sites: &Sites,
    variant: Variant,
    code: &str,
    account_id: Option<&str>,
    report: &mut dyn FnMut(String),
) -> Result<Option<Url>> {
    let jar = Arc::new(Jar::default());
    // Pas de redirection automatique : on veut voir chaque saut et ses cookies.
    let http = reqwest::Client::builder()
        .default_headers(launcher_headers())
        .cookie_provider(jar.clone())
        .redirect(Policy::none())
        .build()?;
    let cosmos_auth = format!("{}/api/cosmos/auth", sites.unreal);

    let Some(sid) = epic_login(&http, &jar, sites, code, report).await? else {
        return Ok(None);
    };

    let full = matches!(variant, Variant::Full);
    let silent = match variant {
        Variant::Full => {
            report("4. unrealengine.com : ouverture de la session (set-sid)".into());
            follow(
                &http,
                &format!("{}/id/api/set-sid?sid={sid}", sites.unreal),
                None,
                None,
                report,
            )
            .await?;
            report(sent_to(&jar, &cosmos_auth)?);
            report("5. unrealengine.com : jeton du site (cosmos/auth)".into());
            let json = cosmos_auth_call(&http, &cosmos_auth, account_id, report).await?;
            let silent = json.get("silentLogin");
            let url = silent
                .and_then(|silent| silent.get("url"))
                .and_then(Value::as_str);
            let Some(url) = url else {
                report("   pas de silentLogin : rien à suivre".into());
                return Ok(None);
            };
            let url = parse(&format!("{}/", sites.unreal))?
                .join(url)
                .map_err(|e| CoreError::Api(format!("silentLogin.url invalide : {e}")))?;
            let fingerprint = silent
                .and_then(|silent| silent.get("fingerprint"))
                .and_then(Value::as_str);
            describe_url(&url, fingerprint, report);
            url
        }
        Variant::NoSetSid(url) => {
            report("4-5. (sautées : ni set-sid ni premier cosmos/auth)".into());
            url
        }
    };

    report("5b. connexion silencieuse".into());
    silent_login(&http, silent.clone(), report).await?;
    report(sent_to(&jar, &cosmos_auth)?);
    report("5c. unrealengine.com : jeton du site, après la connexion silencieuse".into());
    cosmos_auth_call(&http, &cosmos_auth, account_id, report).await?;

    report("7. unrealengine.com : CLUF unreal_engine2".into());
    let response = follow(
        &http,
        &format!(
            "{}/api/cosmos/eula/accept?eulaId=unreal_engine2&locale=en",
            sites.unreal
        ),
        Some(JSON),
        None,
        report,
    )
    .await?;
    let body = response.text().await.unwrap_or_default();
    report(format!("   réponse : {}", excerpt(&body, 300)));

    report("8. unrealengine.com : liste des moteurs (blobs/linux)".into());
    report(sent_to(&jar, &blobs_url(sites))?);
    blobs_call(&http, sites, report).await?;

    if full {
        report("9. unrealengine.com/linux : la page de téléchargement".into());
        linux_page(&http, sites, report).await?;

        report("10. liste des moteurs sans l'ancien cookie EPIC_BEARER_TOKEN de set-sid".into());
        forget_cookie(&jar, &sites.unreal, "EPIC_BEARER_TOKEN")?;
        report(sent_to(&jar, &blobs_url(sites))?);
        blobs_call(&http, sites, report).await?;
        report(sent_to(&jar, &cosmos_auth)?);
        cosmos_auth_call(&http, &cosmos_auth, account_id, report).await?;
    }
    Ok(Some(silent))
}

/// Étapes 1 à 3 : connexion sur epicgames.com avec le code d'échange. Renvoie le SID.
async fn epic_login(
    http: &reqwest::Client,
    jar: &Jar,
    sites: &Sites,
    code: &str,
    report: &mut dyn FnMut(String),
) -> Result<Option<String>> {
    let exchange = format!("{}/id/api/exchange", sites.epic);
    let redirect = format!("{}/id/api/redirect?", sites.epic);

    report("1. epicgames.com : jeton anti-falsification (XSRF)".into());
    follow(
        http,
        &format!("{}/id/api/reputation", sites.epic),
        None,
        None,
        report,
    )
    .await?;
    // Epic pose ce cookie avec Path=/id : on le cherche là où il sera envoyé.
    let xsrf = cookie_value(jar, &parse(&exchange)?, "XSRF-TOKEN");
    report(format!(
        "   XSRF-TOKEN : {}",
        if xsrf.is_some() { "présent" } else { "absent" }
    ));

    report("2. epicgames.com : connexion avec le code d'échange".into());
    let response = http
        .post(&exchange)
        .header("x-xsrf-token", xsrf.unwrap_or_default())
        .json(&serde_json::json!({ "exchangeCode": code }))
        .send()
        .await?;
    let ok = response.status().is_success();
    report(format!(
        "   POST {} → {}",
        place(response.url()),
        response.status()
    ));
    describe(&response, report);
    let body = response.text().await.unwrap_or_default();
    if !ok {
        report(format!("   réponse : {}", excerpt(&body, 300)));
    }

    report("3. epicgames.com : identifiant de session (SID)".into());
    let response = follow(http, &redirect, Some(JSON), None, report).await?;
    let body = response.text().await.unwrap_or_default();
    let json: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let sid = json.get("sid").and_then(Value::as_str).map(str::to_string);
    report(format!(
        "   sid : {}",
        if sid.is_some() { "présent" } else { "absent" }
    ));
    if sid.is_none() {
        describe_redirect(&json, report);
        report("   arrêt : sans SID, la connexion sur epicgames.com a échoué".into());
    }
    Ok(sid)
}

/// Suit l'adresse de connexion silencieuse jusqu'à la page où elle aboutit.
async fn silent_login(
    http: &reqwest::Client,
    url: Url,
    report: &mut dyn FnMut(String),
) -> Result<()> {
    let response = follow_url(http, url, Some(HTML), None, report).await?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    report(format!(
        "   page finale ({status}) : {} octets, titre : {}",
        body.len(),
        title(&body)
    ));
    Ok(())
}

async fn blobs_call(
    http: &reqwest::Client,
    sites: &Sites,
    report: &mut dyn FnMut(String),
) -> Result<()> {
    let response = follow(http, &blobs_url(sites), Some(JSON), None, report).await?;
    let body = response.text().await.unwrap_or_default();
    let json: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    match json.get("blobs").and_then(Value::as_array) {
        Some(blobs) => {
            report(format!("   {} fichier(s) :", blobs.len()));
            for blob in blobs {
                let name = blob.get("name").and_then(Value::as_str).unwrap_or("?");
                report(format!("   - {name}"));
            }
        }
        None => report(format!("   réponse : {}", excerpt(&body, 300))),
    }
    Ok(())
}

/// Charge la page /linux et cherche, dans elle et dans ses scripts, par où passent
/// les téléchargements du moteur.
async fn linux_page(
    http: &reqwest::Client,
    sites: &Sites,
    report: &mut dyn FnMut(String),
) -> Result<()> {
    let response = follow(
        http,
        &format!("{}/linux", sites.unreal),
        Some(HTML),
        None,
        report,
    )
    .await?;
    let page = response.url().clone();
    let status = response.status();
    let html = response.text().await.unwrap_or_default();
    report(format!(
        "   page finale ({status}) : {} octets, titre : {}",
        html.len(),
        title(&html)
    ));
    report(format!("   indices : {}", hints(&html)));
    report(format!(
        "   noms de zip du moteur dans la page : {}",
        list_or_none(&engine_names(&html))
    ));
    let parsed = page::parse_linux_page(&html);
    report(format!(
        "   zip reconnus avec leur adresse : {} (avec taille : {}, avec date : {})",
        parsed.len(),
        parsed.iter().filter(|d| d.size.is_some()).count(),
        parsed.iter().filter(|d| d.created_at.is_some()).count()
    ));
    if let Some(shape) = page::shape_around_first_zip(&html, 400) {
        report(format!(
            "   forme des données autour du premier zip : {shape}"
        ));
    }
    report(format!(
        "   adresses de téléchargement présignées dans la page : {}",
        if html.contains("X-Amz-") || html.contains("Key-Pair-Id") {
            "oui"
        } else {
            "non"
        }
    ));

    let mut paths = api_paths(&html);
    let scripts = script_sources(&html, &page);
    let mut read = 0;
    for src in scripts.iter().take(120) {
        let Ok(response) = http.get(src.clone()).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let js = response.text().await.unwrap_or_default();
        read += 1;
        paths.extend(api_paths(&js));
        for name in engine_names(&js) {
            paths.insert(format!("(nom de zip dans un scripts) {name}"));
        }
    }
    report(format!(
        "   scripts de la page : {} trouvés, {read} lus",
        scripts.len()
    ));
    let wanted = ["blob", "linux", "download", "engine", "eula", "binar"];
    let interesting: Vec<&String> = paths
        .iter()
        .filter(|path| {
            let lower = path.to_lowercase();
            wanted.iter().any(|word| lower.contains(word))
        })
        .collect();
    report(format!(
        "   adresses d'API qui parlent de moteurs ou de téléchargement : {}",
        interesting.len()
    ));
    for path in interesting.iter().take(40) {
        report(format!("   - {path}"));
    }
    Ok(())
}

/// Les chemins "/api/…" écrits dans un texte (HTML ou JavaScript), sans paramètres.
fn api_paths(text: &str) -> BTreeSet<String> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || "_-/.{}$:".contains(c);
    text.match_indices("/api/")
        .map(|(start, _)| {
            let path: String = text[start..]
                .chars()
                .take_while(|c| allowed(*c))
                .take(100)
                .collect();
            path.trim_end_matches(['.', ':']).to_string()
        })
        .filter(|path| path.len() > "/api/".len())
        .collect()
}

/// "Linux_Unreal_Engine_5.8.2.zip" et ses semblables, trouvés dans un texte.
fn engine_names(text: &str) -> BTreeSet<String> {
    let prefix = "Linux_Unreal_Engine_";
    text.match_indices(prefix)
        .filter_map(|(start, _)| {
            let rest = &text[start + prefix.len()..];
            let version: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .take(20)
                .collect();
            let version = version.trim_end_matches('.');
            (!version.is_empty()).then(|| format!("{prefix}{version}"))
        })
        .collect()
}

/// Les adresses des scripts d'une page, en absolu.
fn script_sources(html: &str, page: &Url) -> Vec<Url> {
    let mut found = Vec::new();
    for (start, _) in html.match_indices("src=\"") {
        let rest = &html[start + 5..];
        let Some(end) = rest.find('"') else {
            continue;
        };
        let src = &rest[..end];
        let path = src.split('?').next().unwrap_or(src);
        if !path.ends_with(".js") {
            continue;
        }
        if let Ok(url) = page.join(src)
            && !found.contains(&url)
        {
            found.push(url);
        }
    }
    found
}

fn list_or_none(items: &BTreeSet<String>) -> String {
    if items.is_empty() {
        "aucun".into()
    } else {
        items.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

/// Fait expirer un cookie du site, comme le ferait le serveur avec Max-Age=0.
fn forget_cookie(jar: &Jar, site: &str, name: &str) -> Result<()> {
    let url = parse(&format!("{site}/"))?;
    let host = url.host_str().unwrap_or_default();
    // set-sid pose ses cookies sur le domaine parent (.unrealengine.com) ;
    // pour une adresse IP (les tests), le cookie est lié à l'hôte.
    let domain = if host.parse::<std::net::IpAddr>().is_ok() {
        String::new()
    } else {
        format!("; Domain=.{}", host.strip_prefix("www.").unwrap_or(host))
    };
    jar.add_cookie_str(&format!("{name}=; Max-Age=0; Path=/{domain}"), &url);
    Ok(())
}

/// Appelle cosmos/auth et raconte tous les champs de la réponse.
async fn cosmos_auth_call(
    http: &reqwest::Client,
    url: &str,
    account_id: Option<&str>,
    report: &mut dyn FnMut(String),
) -> Result<Value> {
    let response = follow(http, url, Some(JSON), None, report).await?;
    let body = response.text().await.unwrap_or_default();
    match serde_json::from_str::<Value>(&body) {
        Ok(json) if json.is_object() => {
            describe_fields(&json, account_id, report);
            Ok(json)
        }
        _ => {
            report(format!("   réponse : {}", excerpt(&body, 300)));
            Ok(Value::Null)
        }
    }
}

/// L'adresse de connexion silencieuse : hôte, chemin et paramètres. Seuls les paramètres
/// publics d'OAuth sont affichés en clair ; les autres ne disent que leur longueur.
fn describe_url(url: &Url, fingerprint: Option<&str>, report: &mut dyn FnMut(String)) {
    report(format!("   adresse : {}", place(url)));
    for (key, value) in url.query_pairs() {
        let text = match key.as_ref() {
            "client_id"
            | "response_type"
            | "prompt"
            | "scope"
            | "lang"
            | "display"
            | "response_mode"
            | "code_challenge_method" => value.to_string(),
            "redirect_uri" | "redirectUrl" | "redirect_url" => Url::parse(&value)
                .map(|url| place(&url))
                .unwrap_or_else(|_| format!("texte de {} caractères", value.chars().count())),
            _ => format!("texte de {} caractères", value.chars().count()),
        };
        let same = fingerprint.is_some_and(|fp| !fp.is_empty() && fp == value);
        report(format!(
            "     {key} = {text}{}",
            if same { " (c'est la fingerprint)" } else { "" }
        ));
    }
    if let Some(fingerprint) = fingerprint {
        let inside = url.as_str().contains(fingerprint);
        report(format!(
            "   fingerprint : texte de {} caractères, {} l'adresse",
            fingerprint.chars().count(),
            if inside {
                "présente dans"
            } else {
                "absente de"
            }
        ));
    }
}

/// Quelques mots qui disent ce que fait une page HTML, sans en montrer le contenu.
fn hints(html: &str) -> String {
    let found: Vec<&str> = [
        "postMessage",
        "window.parent",
        "location.replace",
        "location.href",
        "silent",
        "cosmos",
        "fingerprint",
        "<form",
    ]
    .into_iter()
    .filter(|word| html.contains(word))
    .collect();
    if found.is_empty() {
        "aucun".into()
    } else {
        found.join(", ")
    }
}

fn parse(url: &str) -> Result<Url> {
    Url::parse(url).map_err(|e| CoreError::Api(format!("adresse invalide {url} : {e}")))
}

async fn follow(
    http: &reqwest::Client,
    start: &str,
    accept: Option<&str>,
    referer: Option<&str>,
    report: &mut dyn FnMut(String),
) -> Result<Response> {
    follow_url(http, parse(start)?, accept, referer, report).await
}

/// GET qui suit les redirections à la main, en racontant chaque saut.
async fn follow_url(
    http: &reqwest::Client,
    start: Url,
    accept: Option<&str>,
    referer: Option<&str>,
    report: &mut dyn FnMut(String),
) -> Result<Response> {
    let mut url = start;
    for _ in 0..15 {
        let mut request = http.get(url.clone());
        if let Some(accept) = accept {
            request = request.header(ACCEPT, accept);
        }
        if let Some(referer) = referer {
            request = request.header(REFERER, referer);
        }
        let response = request.send().await?;
        report(format!(
            "   GET {}{} → {}",
            place(&url),
            query_names(&url),
            response.status()
        ));
        describe(&response, report);
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| CoreError::Api("redirection sans adresse".into()))?;
        url = url
            .join(location)
            .map_err(|e| CoreError::Api(format!("redirection invalide : {e}")))?;
    }
    Err(CoreError::Api("plus de 15 redirections".into()))
}

/// " [?code, state]" : les noms des paramètres, jamais leurs valeurs.
fn query_names(url: &Url) -> String {
    let names: Vec<String> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
    if names.is_empty() {
        String::new()
    } else {
        format!(" [?{}]", names.join(", "))
    }
}

/// Hôte et chemin, sans les paramètres : ils peuvent contenir un SID ou un code.
fn place(url: &Url) -> String {
    format!("{}{}", url.host_str().unwrap_or("?"), url.path())
}

/// Les cookies posés par la réponse : nom et attributs, jamais la valeur.
fn describe(response: &Response, report: &mut dyn FnMut(String)) {
    for header in response.headers().get_all(SET_COOKIE) {
        if let Ok(text) = header.to_str() {
            report(format!("     Set-Cookie {}", cookie_summary(text)));
        }
    }
}

/// La réponse de /id/api/redirect : le SID et les codes juste « présent »,
/// l'avertissement en clair (c'est un message d'Epic).
fn describe_redirect(json: &Value, report: &mut dyn FnMut(String)) {
    let Some(object) = json.as_object() else {
        report("   réponse non JSON".into());
        return;
    };
    for (key, value) in object {
        let text = match (key.as_str(), value) {
            (_, Value::Null) => "null".to_string(),
            ("warning", Value::String(text)) => format!("« {} »", excerpt(text, 200)),
            ("redirectUrl", Value::String(text)) => Url::parse(text)
                .map(|url| place(&url))
                .unwrap_or_else(|_| format!("texte de {} caractères", text.chars().count())),
            (_, Value::String(_)) => "présent".to_string(),
            (_, other) => kind(other),
        };
        report(format!("   {key} : {text}"));
    }
}

/// Tous les champs d'une réponse JSON : valeur des booléens, nombres et null ;
/// pour le reste, seulement leur forme. L'identifiant de compte est comparé à celui d'uehub.
fn describe_fields(json: &Value, account_id: Option<&str>, report: &mut dyn FnMut(String)) {
    let Some(object) = json.as_object() else {
        return;
    };
    for (key, value) in object {
        let text = match (key.as_str(), value) {
            ("accountId", Value::String(id)) if id.is_empty() => "vide".to_string(),
            ("accountId", Value::String(id)) => {
                if Some(id.as_str()) == account_id {
                    "le même que dans uehub".to_string()
                } else {
                    "différent de celui d'uehub".to_string()
                }
            }
            (_, Value::Bool(_) | Value::Number(_) | Value::Null) => value.to_string(),
            (_, other) => kind(other),
        };
        report(format!("   {key} : {text}"));
    }
}

/// "texte de 12 caractères", "objet {a, b}", "liste de 3 élément(s)"
fn kind(value: &Value) -> String {
    match value {
        Value::String(text) => format!("texte de {} caractères", text.chars().count()),
        Value::Array(items) => format!("liste de {} élément(s)", items.len()),
        Value::Object(_) => format!("objet {{{}}}", keys(value)),
        other => other.to_string(),
    }
}

/// "EPIC_EG1=abc; Domain=.unrealengine.com; Path=/; Secure; HttpOnly"
/// → "EPIC_EG1 (Domain=.unrealengine.com, Path=/, Secure, HttpOnly)"
fn cookie_summary(set_cookie: &str) -> String {
    let mut parts = set_cookie.split(';').map(str::trim);
    let first = parts.next().unwrap_or_default();
    let (name, value) = first.split_once('=').unwrap_or((first, ""));
    let mut attributes: Vec<String> = parts
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            // Ces trois attributs ne contiennent rien de secret et expliquent un refus.
            Some((key, value))
                if ["domain", "path", "samesite"].contains(&key.trim().to_lowercase().as_str()) =>
            {
                format!("{}={}", key.trim(), value.trim())
            }
            Some((key, _)) => key.trim().to_string(),
            None => part.to_string(),
        })
        .collect();
    if value.is_empty() {
        attributes.insert(0, "valeur vide".into());
    }
    format!("{name} ({})", attributes.join(", "))
}

/// "   cookies envoyés à www.epicgames.com/id/api/exchange : XSRF-TOKEN, …"
fn sent_to(jar: &Jar, url: &str) -> Result<String> {
    let url = parse(url)?;
    Ok(format!(
        "   cookies envoyés à {} : {}",
        place(&url),
        jar_names(jar, &url)
    ))
}

/// Les noms des cookies que le client enverrait à cette adresse.
fn jar_names(jar: &Jar, url: &Url) -> String {
    let Some(header) = jar.cookies(url) else {
        return "aucun".into();
    };
    let names: Vec<&str> = header
        .to_str()
        .unwrap_or_default()
        .split(';')
        .filter_map(|pair| pair.trim().split('=').next())
        .filter(|name| !name.is_empty())
        .collect();
    if names.is_empty() {
        "aucun".into()
    } else {
        names.join(", ")
    }
}

fn cookie_value(jar: &Jar, url: &Url, name: &str) -> Option<String> {
    let header = jar.cookies(url)?;
    header
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

fn keys(json: &Value) -> String {
    match json.as_object() {
        Some(object) if !object.is_empty() => object.keys().cloned().collect::<Vec<_>>().join(", "),
        Some(_) => "aucun".into(),
        None => "réponse non JSON".into(),
    }
}

/// Le contenu de <title>, ou "aucun".
fn title(html: &str) -> String {
    let Some(start) = html.find("<title") else {
        return "aucun".into();
    };
    let Some(open_end) = html[start..].find('>') else {
        return "aucun".into();
    };
    let from = start + open_end + 1;
    let Some(len) = html[from..].find("</title>") else {
        return "aucun".into();
    };
    excerpt(&html[from..from + len], 120)
}

/// Les `max` premiers caractères, sur une ligne.
fn excerpt(body: &str, max: usize) -> String {
    let line: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return "(vide)".into();
    }
    match line.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line,
    }
}
