//! La session sur unrealengine.com, ouverte à partir de la connexion uehub.
//!
//! Le parcours qu'Epic accepte (octobre 2026) :
//! 1. epicgames.com pose un jeton anti-falsification (cookie XSRF-TOKEN) ;
//! 2. on s'y connecte avec un code d'échange tiré de la session uehub ;
//! 3. epicgames.com donne un identifiant de session (SID) ;
//! 4. unrealengine.com reçoit ce SID (set-sid) ;
//! 5. unrealengine.com demande une « connexion silencieuse » : on suit l'adresse qu'il
//!    indique, et il pose son propre cookie de session.
//!
//! Depuis le 20 octobre 2026, Cloudflare bloque unrealengine.com pour tout client qui n'a
//! pas l'empreinte TLS et HTTP/2 d'un navigateur, quel que soit son User-Agent. D'où wreq
//! plutôt que reqwest : il se présente comme Firefox.

use std::sync::Arc;

use reqwest::Url;
use serde_json::Value;
use wreq::Response;
use wreq::cookie::Jar;
use wreq::header::ACCEPT;
use wreq::redirect::Policy;
use wreq_util::Emulation;

use super::{EngineDownload, EngineList, ListedEngine, page};
use crate::{CoreError, EpicClient, Result, t};

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
    http: wreq::Client,
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
            .map_err(|e| CoreError::epic(t!("steps.exchange_code"), e))?;
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
        let http = wreq::Client::builder()
            .emulation(Emulation::Firefox151)
            .cookie_provider(jar.clone())
            // wreq ne suit aucune redirection par défaut ; la connexion silencieuse en a
            // besoin. 10, comme reqwest.
            .redirect(Policy::limited(10))
            .connect_timeout(crate::CONNECT_TIMEOUT)
            .read_timeout(crate::READ_TIMEOUT)
            .build()?;

        // 1. Le jeton XSRF est posé sur /id : on le lit là où il sera renvoyé.
        let exchange = format!("{}/id/api/exchange", sites.epic);
        http.get(format!("{}/id/api/reputation", sites.epic))
            .send()
            .await?;
        // Le cookie que le client renverrait à cette adresse (`get` exige le chemin exact).
        let xsrf = jar
            .matches(exchange.as_str())
            .find(|cookie| cookie.name() == "XSRF-TOKEN")
            .map(|cookie| cookie.value().to_string())
            .ok_or_else(|| step(t!("errors.no_xsrf")))?;

        // 2. Connexion sur epicgames.com avec le code d'échange.
        let response = http
            .post(&exchange)
            .header("x-xsrf-token", xsrf)
            .json(&serde_json::json!({ "exchangeCode": code }))
            .send()
            .await?;
        ensure_ok(response, t!("steps.epic_sign_in")).await?;

        // 3. L'identifiant de session.
        let redirect: Value = http
            .get(format!("{}/id/api/redirect", sites.epic))
            .header(ACCEPT, JSON)
            .send()
            .await?
            .json()
            .await?;
        let sid = redirect
            .get("sid")
            .and_then(Value::as_str)
            .ok_or_else(|| step(t!("errors.no_sid")))?;

        // 4. unrealengine.com reçoit la session.
        let response = http
            .get(format!("{}/id/api/set-sid?sid={sid}", sites.unreal))
            .send()
            .await?;
        ensure_ok(response, t!("steps.unreal_session")).await?;

        // 5. La connexion silencieuse, si le site la demande.
        let session = Self { http, sites };
        let auth = session.cosmos_auth().await?;
        if !logged_in(&auth) {
            let silent = auth
                .pointer("/silentLogin/url")
                .and_then(Value::as_str)
                .ok_or_else(|| step(t!("errors.no_silent_login")))?;
            let url = parse(&format!("{}/", session.sites.unreal))?
                .join(silent)
                .map_err(|e| step(t!("errors.bad_silent_url", detail = e)))?;
            let response = session
                .http
                .get(url.as_str())
                .header(ACCEPT, HTML)
                .send()
                .await?;
            ensure_ok(response, t!("steps.silent_login")).await?;
            if !logged_in(&session.cosmos_auth().await?) {
                return Err(step(t!("errors.session_not_recognized")));
            }
        }
        Ok(session)
    }

    async fn cosmos_auth(&self) -> Result<Value> {
        self.http
            .get(format!("{}/api/cosmos/auth", self.sites.unreal))
            .header(ACCEPT, JSON)
            .send()
            .await?
            .json()
            .await
            .map_err(CoreError::from)
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
            .await?;
        let response = ensure_ok(response, t!("steps.eula_check")).await?;
        let json: Value = response.json().await?;
        Ok(json.get("accepted").and_then(Value::as_bool) == Some(true))
    }

    /// Les zip du moteur proposés par la page Linux, du plus récent au plus ancien.
    pub async fn engine_downloads(&self) -> Result<Vec<EngineDownload>> {
        let response = self
            .http
            .get(format!("{}/linux", self.sites.unreal))
            .header(ACCEPT, HTML)
            .send()
            .await?;
        let response = ensure_ok(response, t!("steps.linux_page")).await?;
        let html = response.text().await?;
        let downloads = page::parse_linux_page(&html);
        if downloads.is_empty() {
            return Err(step(t!("errors.no_engine_zip")));
        }
        Ok(downloads)
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

    /// Le zip d'une version précise, avec une adresse de téléchargement fraîche.
    pub async fn engine_download(&self, version: &str) -> Result<EngineDownload> {
        self.engine_downloads()
            .await?
            .into_iter()
            .find(|d| d.version == version)
            .ok_or_else(|| step(t!("errors.engine_not_offered", version = version)))
    }
}

/// Le site a-t-il reconnu le compte ?
fn logged_in(auth: &Value) -> bool {
    auth.pointer("/account/isLoggedIn").and_then(Value::as_bool) == Some(true)
}

/// Une étape qui n'a pas marché, sans réponse d'Epic à citer.
fn step(message: String) -> CoreError {
    CoreError::Api(message)
}

fn parse(url: &str) -> Result<Url> {
    Url::parse(url).map_err(|e| step(t!("errors.bad_url", detail = e)))
}

/// Un statut d'erreur devient une erreur qui dit l'étape et le statut. La réponse
/// d'Epic ne va que dans le journal ; l'adresse appelée, nulle part.
async fn ensure_ok(response: Response, step: String) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    // Ce n'est pas Epic qui refuse, c'est Cloudflare qui veut un navigateur : le dire,
    // sinon on cherche le problème du côté du compte.
    if blocked_by_cloudflare(&response) {
        return Err(CoreError::Api(t!("errors.cloudflare", step = step)));
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(CoreError::Epic {
        step,
        status: Some(status.to_string()),
        detail: body.chars().take(300).collect(),
    })
}

/// Cloudflare marque ainsi la page de vérification qu'il sert à la place du site.
fn blocked_by_cloudflare(response: &Response) -> bool {
    response
        .headers()
        .get("cf-mitigated")
        .is_some_and(|value| value == "challenge")
}

#[cfg(test)]
mod tests;
