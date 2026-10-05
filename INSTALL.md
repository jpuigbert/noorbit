# NoOrbit — Guia de construcció i instal·lació

NoOrbit és una aplicació **Tauri 2** (escriptori natiu + interfície web). Es pot
empaquetar per a tres famílies de sistemes. Cada instal·lador s'ha de **construir
des del seu sistema operatiu** (o en CI): des de macOS no es poden crear de forma
fiable binaris `.deb` de Linux ni `.exe` de Windows, perquè enllacen biblioteques
pròpies d'aquelles plataformes. El que sí es pot fer des d'un sol Mac és produir un
**DMG universal** que funcioni tant en Mac Intel com en Apple Silicon.

## Requisits comuns (a totes les plataformes)

| Eina | Versió mínima | Nota |
|------|----------------|------|
| [Node.js](https://nodejs.org) | 20 | amb `corepack` o `pnpm ≥ 9` |
| [Rust](https://rustup.rs) | 1.77 | via `rustup` |
| [pnpm](https://pnpm.io) | 9 | `corepack enable` |

Després de clonar el repositori, instal·la les dependències:

```bash
pnpm install
```

---

## 1. macOS — instal·lador **universal** (Intel + Apple Silicon)

Aquest és el paquet consistent per a **qualsevol Mac modern**, i s'instal·la també
en màquines Intel.

### Construir

```bash
# Afegeix els dos targets de Rust (només cal un cop)
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# DMG + .app universal (una sola comanda)
pnpm app:build:mac:universal
```

Altres opcions si només vols una arquitectura (més ràpides):

```bash
pnpm app:build:mac:intel   # només x86_64 (aquest Mac)
pnpm app:build:mac:arm     # només aarch64 (Apple Silicon)
```

### Resultat

```
apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/
├── dmg/NoOrbit_0.5.0_universal.dmg   ← comparteix això
└── macos/NoOrbit.app
```

### Instal·lar (usuari final)

1. Obre el `.dmg` i arrossega **NoOrbit** a la carpeta **Aplicacions**.
2. La primera vegada, com que l'app **no està signada ni notaritzada**, macOS la
   bloqueja. Fes **clic dret → Obre** (una sola vegada) o, per terminal:

   ```bash
   xattr -dr com.apple.quarantine /Applications/NoOrbit.app
   ```

> Per distribuir-ho públicament sense aquest avís, cal signar amb un certificat Apple
> Developer i notaritzar (`pnpm tauri build` amb les variables d'entorn de signatura).

---

## 2. GNU/Linux — Debian / Ubuntu (`.deb`)

### Construir **des d'una Debian/Ubuntu**

Instal·la primer les dependències del sistema (WebkitGTK i companyia):

```bash
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Després, al repositori:

```bash
pnpm install
pnpm app:build:deb          # només .deb
# pnpm app:build:linux      # .deb + .rpm + AppImage (si tens les eines)
```

### Resultat

```
apps/desktop/src-tauri/target/release/bundle/deb/no-orbit_0.5.0_amd64.deb
```

### Instal·lar (usuari final)

```bash
sudo apt install ./no-orbit_0.5.0_amd64.deb
# o bé, després d'haver-lo descarregat:
sudo dpkg -i no-orbit_0.5.0_amd64.deb && sudo apt -f install
```

Executa'l amb `no-orbit` o des del menú d'aplicacions.

> Nota d'arquitectura: en una ARM Linux (p. ex. Raspberry Pi) afegeix
> `--target aarch64-unknown-linux-gnu` i ajusta el nom del paquet.

---

## 3. Windows — instal·lador típic (`.exe` NSIS + `.msi`)

### Construir **des de Windows**

Requisits:

1. **Microsoft C++ Build Tools** (workload *Desktop development with C++*).
2. **WebView2 Runtime** (ve inclòs en Windows 11; en 10 es baixa sol gràcies a
   `webviewInstallMode: downloadBootstrapper`).
3. **Rust** (eina `msvc`) i Node + pnpm com a dalt.

Després:

```powershell
pnpm install
pnpm app:build:win
```

### Resultat

```
apps\desktop\src-tauri\target\release\bundle\
├── nsis\NoOrbit_0.5.0_x64-setup.exe   ← instal·lador clàssic (recomanat)
└── msi\NoOrbit_0.5.0_x64_en-US.msi     ← paquet per a desplegaments empresarials
```

### Instal·lar (usuari final)

Doble clic al `*-setup.exe`; segueix l'assistent. S'instal·la **per a l'usuari
actual** (sense drets d'administrador). El `.msi` serveix per a desplegaments
massius (GPO/SCCM).

---

## Resum de comandes

| Objectiu | Comanda | On s'executa |
|----------|---------|--------------|
| Mac universal (DMG) | `pnpm app:build:mac:universal` | macOS |
| Mac només Intel | `pnpm app:build:mac:intel` | macOS |
| Mac només Apple Silicon | `pnpm app:build:mac:arm` | macOS |
| Debian `.deb` | `pnpm app:build:deb` | Debian/Ubuntu |
| Linux complet | `pnpm app:build:linux` | Linux |
| Windows `.exe`/`.msi` | `pnpm app:build:win` | Windows |

Tots els paquets surten a `apps/desktop/src-tauri/target/<triple>/release/bundle/`.
