---
name: Desenvolupament per Mac (Swift/Xcode)
description: Programar aplicacions i eines natives per macOS amb Swift — projectes Xcode, SwiftUI, AppKit, App Store i homebrew.
---

# Com programar per macOS amb NoOrbit

Quan l'usuari demani una app, eina o script per al Mac, treballa així:

1. **Tria el format**: app gràfica (SwiftUI), utilitat de terminal (CLI en
   Swift o zsh), AppleScript d'automatització o servei de fons (launchd).
2. **Estructura de l'app SwiftUI** (escriu-la amb «@file:», s'aplica sola):
   - `App.swift` (punt d'entrada amb `@main`)
   - `Models/`, `Views/`, `Views/Components/`
   - `Info.plist` (permisos: `NSAppleEventsUsageDescription` si fas AppleScript)
3. **Compilació sense l'IDE Xcode**, amb Package.swift:
   ```swift
   // swift-tools-version:5.9
   import PackageDescription
   let package = Package(
     name: "Eina",
     targets: [.executableTarget(name: "Eina", path: "Sources")]
   )
   ```
   i després `swift build -c release` (binaris a `.build/release`).
4. **Amb Xcode**: `open project.xcodeproj` i Run (⌘R); també es pot executar
   `xcodebuild -configuration Release build` des del terminal integrat.
5. **Distribució**: `codesign --force --deep -s - Eina.app` per al Mac;
   `create-dmg` per a l'instal·lador. Per compartir-la amb qualsevol persona
   cal notarització (Apple Developer Program).
6. **Convenis de Mac**: barra de menú, mesures retina (@2x), dreceres amb ⌘,
   Apple Events per automatitzar altres apps (mai sense permís explícit).

Recorda: sempre «@file: ruta» per escriure els fitxers, i executa al terminal
amb «@run: swift build» si tens permís d'ordinador actiu.
