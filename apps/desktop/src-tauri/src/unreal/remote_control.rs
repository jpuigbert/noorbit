//! Arrencada i aturada dels servidors de Remote Control d'Unreal.
//!
//! La Remote Control API cal activar-la normalment a mà
//! (Window → Remote Control). Aquest mòdul ho fa programàticament
//! cridant els CDO de `RemoteControlHttpServer` i
//! `RemoteControlWebsocketServer` via `/remote/object/call`.

use super::client::UnrealClient;
use anyhow::Result;

pub const HTTP_PORT: u16 = 30010;
pub const WS_PORT: u16 = 30020;

async fn call_server_cdo(
    client: &UnrealClient,
    server: &str,
    method: &str,
    port: Option<u16>,
) -> Result<serde_json::Value> {
    let params = match port {
        Some(p) => serde_json::json!({ "Port": p }),
        None => serde_json::json!({}),
    };
    // L'objecte per defecte (CDO) dels servidors RC admet Start/Stop Server.
    client
        .call_function(
            &format!("/Script/RemoteControl.Default__{}Server", server),
            if method == "start" { "StartServer" } else { "StopServer" },
            params,
        )
        .await
}

/// Obre el servidor HTTP de Remote Control al port indicat.
pub async fn start_http(client: &UnrealClient, port: u16) -> Result<serde_json::Value> {
    call_server_cdo(client, "Http", "start", Some(port)).await
}

/// Obre el servidor WebSocket de Remote Control al port indicat.
pub async fn start_websocket(client: &UnrealClient, port: u16) -> Result<serde_json::Value> {
    call_server_cdo(client, "Websocket", "start", Some(port)).await
}

/// Tanca el servidor HTTP.
pub async fn stop_http(client: &UnrealClient) -> Result<serde_json::Value> {
    call_server_cdo(client, "Http", "stop", None).await
}

/// Tanca el servidor WebSocket.
pub async fn stop_websocket(client: &UnrealClient) -> Result<serde_json::Value> {
    call_server_cdo(client, "Websocket", "stop", None).await
}
