//! Suport per a desenvolupament MÒBIL NATIU: Android (Kotlin/Jetpack Compose)
//! i iOS (Swift/SwiftUI). Detecció de SDKs, gestió d'emuladors/hipersemacions
//! i captures en viu perquè l'usuari puga VEURE el que la IA genera executant-se
//! en un dispositiu virtual, sense eixir de NoOrbit.
//!
//! Tot és NATIU: NoOrbit no embolcalla cap tecnologia web (Capacitor/Cordova/
//! Flutter). La IA escriu fitxers `.kt`/`.swift` reals dins l'estructura de
//! Gradle o Xcode, i els SDKs oficials (Android SDK + Xcode/simctl) fan la
//! resta. Ací només cal detectar-los i controlar-los.

pub mod android;
pub mod commands;
pub mod ios;

pub use android::{AndroidAvd, AndroidStatus};
pub use ios::{IosDevice, IosRuntimeGroup, IosStatus};

use serde::Serialize;

/// Estat conjunt: disponibilitat d'ambdues plataforms.
#[derive(Debug, Clone, Serialize)]
pub struct MobileStatus {
    pub android: AndroidStatus,
    pub ios: IosStatus,
    /// Sistema operatiu actual (iOS Simulator només funciona a macOS).
    pub os: String,
}

impl MobileStatus {
    pub fn current() -> Self {
        Self {
            android: android::detect(),
            ios: ios::detect(),
            os: std::env::consts::OS.to_string(),
        }
    }
}
