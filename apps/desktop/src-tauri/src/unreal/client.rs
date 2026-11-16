//! Client HTTP per a la Remote Control API d'Unreal Engine.
//!
//! Endpoints reals de l'editor (documentació oficial «Remote Control API HTTP
//! Reference»): `GET /remote/info`, `PUT /remote/object/property`,
//! `PUT /remote/object/call`, `PUT /remote/search/assets`, `PUT /remote/batch`.
//!
//! NOTA de camps JSON: la documentació i tots els clients de la comunitat
//! empren noms **camelCase** (`objectPath`, `functionName`, `parameters`,
//! `propertyName`, `propertyValue`). Abans enviàvem `ObjectPath`/`FunctionName`
//! (PascalCase): segons la versió de l'editor el servidor és o no és
//! tolerant amb la caixa, i quan no ho és contesta 400 i NO s'executa res —
//! l'error arribava com a «no passa res a Unreal». Els únics noms que SÓN en
//! PascalCase són els dels PARÀMETRES de la funció (`PythonCommand`), perquè
//! aquests venen del nom reflectit del C++.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = crate::ports::UNREAL_RC_PORT;

/// Objecte per defecte (CDO) de la llibreria de scripts Python: per aqui es
/// crida `ExecutePythonCommand` / `ExecutePythonCommandEx`.
const PYTHON_SCRIPT_CDO: &str = "/Script/PythonScriptPlugin.Default__PythonScriptLibrary";

/// Retalla un text per a els missatges d'error (les respostes RC poden ser llargues).
fn clip(s: &str, n: usize) -> String {
    let t: String = s.trim().chars().take(n).collect();
    if s.trim().chars().count() > n {
        format!("{}…", t)
    } else {
        t
    }
}

/// Temps màxim d'una crida: un script d'import, un `build_light_maps` o un
/// render poden tardar minuts DINS de l'editor. Amb 30 s la petició tallava
/// abans d'acabar i la UI ho reportava com a error (o com a èxit buit).
const REQUEST_TIMEOUT_SECS: u64 = 300;

/// Port REAL de la Remote Control: el que l'usuari hagi posat a la
/// configuració (`unreal.rc_port`), amb el de `ports.rs` com a reserva.
/// La UI no sempre el coneix (envia el per defecte), per això el servidor
/// és qui el resol.
pub fn configured_port() -> u16 {
    crate::config::AppConfig::load()
        .ok()
        .map(|c| c.unreal.rc_port)
        .filter(|p| *p != 0)
        .unwrap_or(DEFAULT_PORT)
}

/// Port efectiu a partir del que demana la UI: si és el per defecte (o 0),
/// mana la configuració.
pub fn resolve_port(requested: u16) -> u16 {
    if requested == 0 || requested == DEFAULT_PORT {
        configured_port()
    } else {
        requested
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteObjectRequest {
    #[serde(rename = "objectPath")]
    pub object_path: String,
    #[serde(rename = "functionName", skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    #[serde(rename = "parameters", skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(rename = "generateTransaction", skip_serializing_if = "Option::is_none")]
    pub generate_transaction: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyRequest {
    #[serde(rename = "objectPath")]
    pub object_path: String,
    #[serde(rename = "propertyName")]
    pub property_name: String,
    #[serde(rename = "access", skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrealInfo {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub engine_version: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
}

pub struct UnrealClient {
    host: String,
    port: u16,
    http: reqwest::Client,
}

impl UnrealClient {
    pub fn new() -> Self {
        Self::with_port(configured_port())
    }

    pub fn with_port(port: u16) -> Self {
        Self {
            host: DEFAULT_HOST.into(),
            port,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Port pel qual aquest client parla realment amb l'editor.
    pub fn port(&self) -> u16 {
        self.port
    }

    fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Comprova si l'editor està en marxa i la Remote Control API respon.
    pub async fn ping(&self) -> Result<bool> {
        let url = format!("{}/remote/info", self.base_url());
        match self.http.get(&url).send().await {
            Ok(r) => Ok(r.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Obté informació de l'editor.
    pub async fn info(&self) -> Result<UnrealInfo> {
        let url = format!("{}/remote/info", self.base_url());
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("UE5 info: {}", resp.status()));
        }
        Ok(resp.json::<UnrealInfo>().await.unwrap_or(UnrealInfo {
            version: None,
            engine_version: None,
            project_name: None,
        }))
    }

    /// Llegeix una propietat d'un objecte.
    pub async fn get_property(
        &self,
        object_path: &str,
        property_name: &str,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/remote/object/property", self.base_url());
        let body = PropertyRequest {
            object_path: object_path.into(),
            property_name: property_name.into(),
            access: Some("READ_ACCESS".into()),
        };
        let resp = self.http.put(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("UE5 get_property: {}", resp.status()));
        }
        Ok(resp.json().await?)
    }

    /// Escriu una propietat d'un objecte.
    pub async fn set_property(
        &self,
        object_path: &str,
        property_name: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        let url = format!("{}/remote/object/property", self.base_url());
        let body = serde_json::json!({
            "objectPath": object_path,
            "propertyName": property_name,
            "access": "WRITE_ACCESS",
            "propertyValue": value,
        });
        let resp = self.http.put(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("UE5 set_property: {}", resp.status()));
        }
        Ok(())
    }

    /// Crida una funció d'un objecte i retorna la resposta crua de l'editor.
    async fn call_raw(&self, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/remote/object/call", self.base_url());
        let resp = self.http.put(&url).json(&body).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("UE5 remote call: {} — {}", status, clip(&text, 400)));
        }
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("UE5 remote call: resposta no és JSON vàlid ({})", e))
    }

    /// Crida una funció d'un objecte.
    pub async fn call_function(
        &self,
        object_path: &str,
        function_name: &str,
        parameters: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let body = RemoteObjectRequest {
            object_path: object_path.into(),
            function_name: Some(function_name.into()),
            parameters: Some(parameters),
            generate_transaction: Some(true),
        };
        self.call_raw(serde_json::to_value(&body)?).await
    }

    /// Executa codi Python dins l'editor via Remote Control.
    /// Requereix el plugin Python Editor Script.
    ///
    /// IMPORTANT: la resposta d'aquesta crida NO conté la sortida del script
    /// ni les seves excepcions — segons la versió de l'editor
    /// `ExecutePythonCommand` retorna `void` (5.0-5.3) o un `bool`
    /// (5.4+) i l'error real queda al log de l'editor. Per això
    /// `python::run_script` embolica el codi amb un script que escriu la
    /// sortida a un fitxer temporal: aquesta funció només és el transport.
    pub async fn execute_python(&self, code: &str) -> Result<serde_json::Value> {
        let call = |name: &str, extra: serde_json::Value| {
            let mut params = serde_json::json!({ "PythonCommand": code });
            if let Some(map) = extra.as_object() {
                for (k, v) in map {
                    params[k] = v.clone();
                }
            }
            serde_json::json!({
                "objectPath": PYTHON_SCRIPT_CDO,
                "functionName": name,
                "parameters": params,
                // Un script Python ja gestiona les seves pròpies transaccions;
                // embolicar-lo en una de «Remote Call Transaction Wrap» només
                // afegeix estat d'undo estrany a l'editor.
                "generateTransaction": false,
            })
        };
        match self.call_raw(call("ExecutePythonCommand", serde_json::json!({}))).await {
            Ok(v) => Ok(v),
            Err(e) => {
                // Versions futures de l'editor podrien no tenir ja la variant
                // simple: provem `ExecutePythonCommandEx` (que a més retorna la
                // sortida capturada) abans de donar-la per perduda.
                let msg = e.to_string().to_lowercase();
                if msg.contains("functionname") || msg.contains("not found") || msg.contains("unknown")
                {
                    self.call_raw(call(
                        "ExecutePythonCommandEx",
                        serde_json::json!({ "ExecutionMode": "EXECUTE_STATEMENT" }),
                    ))
                    .await
                    .map_err(|e2| anyhow!("{} (intent Ex tampoc ha funcionat: {})", e, e2))
                } else {
                    Err(e)
                }
            }
        }
    }
}

impl Default for UnrealClient {
    fn default() -> Self {
        Self::new()
    }
}
