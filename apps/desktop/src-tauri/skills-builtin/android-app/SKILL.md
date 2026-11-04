---
name: Apps Android (Kotlin + Gradle)
description: Crear aplicàcions per Android — Kotlin, Jetpack Compose, Gradle, APK/AAB i Play Store.
---

# Com programar per Android amb NoOrbit

1. **Base**: Kotlin + Jetpack Compose (moderna) o Views XML (clàssica).
   Requereix JDK 17, Android SDK i Gradle (o Android Studio per l'IDE).
2. **Estructura del projecte** (escriu-la amb «@file:»):
   - `build.gradle.kts` + `settings.gradle.kts` (nivell arrel i mòdul `app/`)
   - `app/src/main/AndroidManifest.xml` (permisos, icona, activitat arrel)
   - `app/src/main/java/com/<paquet>/MainActivity.kt` (Compose `setContent { }`)
   - `app/src/main/res/` (strings, icons, temes)
3. **Gradle mínim del mòdul**:
   ```kotlin
   plugins { id("com.android.application"); id("org.jetbrains.kotlin.android") }
   android {
     namespace = "com.exemple.app"
     compileSdk = 34
     defaultConfig { applicationId = "com.exemple.app"; minSdk = 24; targetSdk = 34 }
     buildFeatures { compose = true }
   }
   dependencies { implementation("androidx.compose.material3:material3") }
   ```
4. **Compilar i provar**: `./gradlew assembleDebug` (APK a
   `app/build/outputs/apk/debug/`); instal·lar al mòbil/enulator amb
   `adb install app-debug.apk`; enulator amb `~/Android/Sdk/emulator/emulator
   -avd Test_Api34` (o crear-lo amb avdmanager).
5. **A Google Play**: `.aab` amb `./gradlew bundleRelease` + firma keystore
   (`keytool -genkey -v -keystore release.jks`),usuari puja a la Play Console.
6. **Patrons**: `LaunchedEffect` per comptadors/temps, `rememberSaveable`
   per l'estat que sobreviu girs, `ViewModel` per la lògica que sobreviu a
   rotacions, `Coroutines` per la xarxa (no bloquejar el fil principal/ANI).

No inventis versions de deps: indica «comprova la més recent» o usa les que
l'usuari aguanti al projecte.
