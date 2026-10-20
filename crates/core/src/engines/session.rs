//! La session sur unrealengine.com, ouverte à partir de la connexion uehub.
//!
//! Le parcours qu'Epic accepte (octobre 2026) :
//! 1. epicgames.com pose un jeton anti-falsification (cookie XSRF-TOKEN) ;
//! 2. on s'y connecte avec un code d'échange tiré de la session uehub ;
//! 3. epicgames.com donne un identifiant de session (SID) ;
//! 4. unrealengine.com reçoit ce SID (set-sid) ;
//! 5. unrealengine.com demande une « connexion silencieuse » : on suit l'adresse qu'il
//!    indique, et il pose son propre cookie de session.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use reqwest::{Response, Url};
use serde_json::Value;

use super::{EngineDownload, EngineList, ListedEngine, net, page};
use crate::{CoreError, EpicClient, Result};

const JSON: &str = "application/json";
const HTML: &str = "text/html,application/xhtml+xml";

/// Les adresses des deux sites. Les tests les remplacent par un faux serveur local.
pub(crate) struct Sites {
    pub epic: String,
    pub unreal: String,
}

impl Sites {
    fn real() -> Self {
        Self {
            epic: "https://www.epicgames.com".into(),
            unreal: "https://www.unrealengine.com".into(),
        }
    }
}

/// Une session ouverte sur unrealengine.com.
pub struct UnrealSession {
    http: reqwest::Client,
    sites: Sites,
}

impl EpicClient {
    /// Un code d'échange à usage unique, tiré de la session uehub.
    pub async fn exchange_code(&mut self) -> Result<String> {
        self.resume().await?;
        let token = self
            .egs
            .try_game_token()
            .await
            .map_err(|e| CoreError::Api(format!("code d'échange : {e}")))?;
        Ok(token.code)
    }

    /// Ouvre une session sur unrealengine.com : la page des moteurs et le CLUF passent
    /// par ce site, pas par l'API du launcher qu'on utilise pour Fab.
    pub async fn open_unreal_session(&mut self) -> Result<UnrealSession> {
        let code = self.exchange_code().await?;
        UnrealSession::connect(&code).await
    }
}

impl UnrealSession {
    /// Ouvre la session avec un code d'échange. Pas besoin du client : l'app ne le
    /// verrouille que le temps d'obtenir le code.
    pub async fn connect(code: &str) -> Result<Self> {
        Self::open(Sites::real(), code).await
    }

    pub(crate) async fn open(sites: Sites, code: &str) -> Result<Self> {
        let jar = Arc::new(Jar::default());
        let http = reqwest::Client::builder()
            .default_headers(launcher_headers())
            .cookie_provider(jar.clone())
            .build()?;

        // 1. Le jeton XSRF est posé sur /id : on le lit là où il sera renvoyé.
        let exchange = format!("{}/id/api/exchange", sites.epic);
        http.get(format!("{}/id/api/reputation", sites.epic))
            .send()
            .await
            .map_err(net)?;
        let xsrf = cookie_value(&jar, &parse(&exchange)?, "XSRF-TOKEN")
            .ok_or_else(|| step("epicgames.com n'a pas donné de jeton XSRF"))?;

        // 2. Connexion sur epicgames.com avec le code d'échange.
        let response = http
            .post(&exchange)
            .header("x-xsrf-token", xsrf)
            .json(&serde_json::json!({ "exchangeCode": code }))
            .send()
            .await
            .map_err(net)?;
        ensure_ok(response, "connexion sur epicgames.com").await?;

        // 3. L'identifiant de session.
        let redirect: Value = http
            .get(format!("{}/id/api/redirect", sites.epic))
            .header(ACCEPT, JSON)
            .send()
            .await
            .map_err(net)?
            .json()
            .await
            .map_err(net)?;
        let sid = redirect
            .get("sid")
            .and_then(Value::as_str)
            .ok_or_else(|| step("epicgames.com n'a pas donné d'identifiant de session"))?;

        // 4. unrealengine.com reçoit la session.
        let response = http
            .get(format!("{}/id/api/set-sid?sid={sid}", sites.unreal))
            .send()
            .await
            .map_err(net)?;
        ensure_ok(response, "ouverture de la session unrealengine.com").await?;

        // 5. La connexion silencieuse, si le site la demande.
        let session = Self { http, sites };
        let auth = session.cosmos_auth().await?;
        if !logged_in(&auth) {
            let silent = auth
                .pointer("/silentLogin/url")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    step("unrealengine.com refuse la session sans proposer d'autre voie")
                })?;
            let url = parse(&format!("{}/", session.sites.unreal))?
                .join(silent)
                .map_err(|e| step(&format!("adresse de connexion silencieuse invalide : {e}")))?;
            let response = session
                .http
                .get(url)
                .header(ACCEPT, HTML)
                .send()
                .await
                .map_err(net)?;
            ensure_ok(response, "connexion silencieuse sur unrealengine.com").await?;
            if !logged_in(&session.cosmos_auth().await?) {
                return Err(step(
                    "unrealengine.com ne reconnaît pas la session après la connexion silencieuse",
                ));
            }
        }
        Ok(session)
    }

    async fn cosmos_auth(&self) -> Result<Value> {
        self.http
            .get(format!("{}/api/cosmos/auth", self.sites.unreal))
            .header(ACCEPT, JSON)
            .send()
            .await
            .map_err(net)?
            .json()
            .await
            .map_err(net)
    }

    /// Le CLUF d'Unreal Engine est-il accepté par ce compte ?
    pub async fn eula_accepted(&self) -> Result<bool> {
        let response = self
            .http
            .get(format!(
                "{}/api/cosmos/eula/accept?eulaId=unreal_engine2&locale=en",
                self.sites.unreal
            ))
            .header(ACCEPT, JSON)
            .send()
            .await
            .map_err(net)?;
        let response = ensure_ok(response, "vérification du CLUF").await?;
        let json: Value = response.json().await.map_err(net)?;
        Ok(json.get("accepted").and_then(Value::as_bool) == Some(true))
    }

    /// La liste des moteurs proposés, sans les adresses de téléchargement. Elle est
    /// gardée sur le disque : le prochain affichage n'aura pas à attendre Epic.
    pub async fn engine_list(&self) -> Result<EngineList> {
        let list = self.read_engine_list().await?;
        list.save();
        Ok(list)
    }

    /// La même liste, sans la garder : les tests la lisent sans toucher à ton cache.
    async fn read_engine_list(&self) -> Result<EngineList> {
        let downloads = self.engine_downloads().await?;
        Ok(EngineList::checked_now(
            downloads.iter().map(ListedEngine::from).collect(),
        ))
    }

    /// Les zip du moteur proposés par la page Linux, du plus récent au plus ancien.
    pub async fn engine_downloads(&self) -> Result<Vec<EngineDownload>> {
        let response = self
            .http
            .get(format!("{}/linux", self.sites.unreal))
            .header(ACCEPT, HTML)
            .send()
            .await
            .map_err(net)?;
        let response = ensure_ok(response, "page Linux").await?;
        let html = response.text().await.map_err(net)?;
        let downloads = page::parse_linux_page(&html);
        if downloads.is_empty() {
            return Err(step(
                "la page Linux ne contient aucun zip du moteur : vérifie que le CLUF est accepté",
            ));
        }
        Ok(downloads)
    }

    /// Le zip d'une version précise, avec une adresse de téléchargement fraîche.
    pub async fn engine_download(&self, version: &str) -> Result<EngineDownload> {
        self.engine_downloads()
            .await?
            .into_iter()
            .find(|d| d.version == version)
            .ok_or_else(|| step(&format!("UE {version} n'est pas proposé sur la page Linux")))
    }
}

/// Les en-têtes du launcher Epic, comme egs-api : Epic les accepte aussi sur ses sites web.
fn launcher_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "UELauncher/17.0.1-37584233+++Portal+Release-Live Windows/10.0.19043.1.0.64bit",
        ),
    );
    headers
}

/// Le site a-t-il reconnu le compte ?
fn logged_in(auth: &Value) -> bool {
    auth.pointer("/account/isLoggedIn").and_then(Value::as_bool) == Some(true)
}

fn step(message: &str) -> CoreError {
    CoreError::Api(message.to_string())
}

fn parse(url: &str) -> Result<Url> {
    Url::parse(url).map_err(|e| step(&format!("adresse invalide : {e}")))
}

/// Un statut d'erreur devient une erreur qui dit l'étape et la réponse d'Epic,
/// sans l'adresse appelée.
async fn ensure_ok(response: Response, what: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let body: String = body.chars().take(300).collect();
    Err(step(&format!("{what} : {status} {body}")))
}

/// La valeur d'un cookie que le client renverrait à cette adresse.
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
