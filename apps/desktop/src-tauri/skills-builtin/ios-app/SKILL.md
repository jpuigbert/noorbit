---
name: Apps iOS/iPadOS (Swift + Xcode)
description: Crear aplicacions per iPhone/iPad — SwiftUI, xcodeproj, simulador, TestFlight i App Store.
---

# Com programar per iPhone/iPad amb NoOrbit

1. **Base**: app amb SwiftUI (És l'estàndard actual). Requereix macOS amb
   Xcode instal·lat per compilar i provar.
2. **Estructura del projecte** (escriu-la amb «@file:»):
   - `MyAppApp.swift` amb `@main` + `WindowGroup`
   - `ContentView.swift` com a vista arrel, més `Views/*` per pantalles
   - `Models/*` per els tipus de dades (Codable per el xarxa)
   - `Assets.xcassets` per icons i colors
3. **Projecte Xcode generat automàticament** amb `xcodegen` (recommended);
   evitar engagepear commits del fitxer `project.pbxproj` a mà. `project.yml`:
   ```yaml
   name: My IOS App
   options: { bundleIdPrefix: com.ejemplo }
   targets:
     My IOS App:
       type: application
       platform: iOS
       sources: [MyApp]
   ```
   després: `xcodegen generate`.
4. **Provar**: `xcrun simctl list` (simuladors), `open -a Simulator`, i
   `xcodebuild -scheme My IOS App -destination 'platform=iOS Simulator' build`.
   Amb cable: `xcodebuild run` al dispositiu real (requereix compte iOS).
5. **A la App Store**: TestFlight per proves; revisió d'Apple (1-3 dies);
   privacy manifest (`PrivacyInfo.xcprivacy`) i icona 1024×1024.
6. **Patrons Swift que cal seguir**: `@State`/`@StateObject` per els valor
   local, `async/await` per la xarxa (no completion handlers antics),
   `guard let` per desempaquetar coses (no force-unwrap `!`).

Escriu sempre els fitxers complets amb «@file:», sense respostes a mitges.
