//! Client HTTP per a la Remote Control API d'Unreal Engine.
//!
//! Endpoints principals:
//! - GET  /remote/object/property
//! - PUT  /remote/object/property
//! - PUT  /remote/object/call
//! - GET  /remote/info
//!
//! El port per defecte és 30010.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 30010;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteObjectRequest {
    #[serde(rename = "ObjectPath")]
    pub object_path: String,
    #[serde(rename = "FunctionName", skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    #[serde(rename = "Parameters", skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(rename = "GenerateTransaction", skip_serializing_if = "Option::is_none")]
    pub generate_transaction: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyRequest {
    #[serde(rename = "ObjectPath")]
    pub object_path: String,
    #[serde(rename = "propertyName")]
    pub property_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        Self {
            host: DEFAULT_HOST.into(),
            port: DEFAULT_PORT,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_port(port: u16) -> Self {
        Self {
            host: DEFAULT_HOST.into(),
            port,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
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
            "ObjectPath": object_path,
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

    /// Crida una funció d'un objecte.
    pub async fn call_function(
        &self,
        object_path: &str,
        function_name: &str,
        parameters: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/remote/object/call", self.base_url());
        let body = RemoteObjectRequest {
            object_path: object_path.into(),
            function_name: Some(function_name.into()),
            parameters: Some(parameters),
            generate_transaction: Some(true),
        };
        let resp = self.http.put(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("UE5 call_function: {}", resp.status()));
        }
        Ok(resp.json().await?)
    }

    /// Executa codi Python dins l'editor via Remote Control.
    /// Requereix el plugin Python Editor Script.
    pub async fn execute_python(&self, code: &str) -> Result<serde_json::Value> {
        let url = format!("{}/remote/object/call", self.base_url());
        let body = serde_json::json!({
            "ObjectPath": "/Script/PythonScriptPlugin.Default__PythonScriptLibrary",
            "FunctionName": "ExecutePythonCommand",
            "Parameters": {
                "PythonCommand": code,
            },
            "GenerateTransaction": true,
        });
        let resp = self.http.put(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("UE5 Python: {} — {}", status, text));
        }
        Ok(resp.json().await?)
    }
}

impl Default for UnrealClient {
    fn default() -> Self {
        Self::new()
    }
}
