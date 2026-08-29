# MorseLink — Cross-Platform P2P File Transfer & Media Suite

A decentralized, peer-to-peer file transfer application (Android APK + PC
companion + Web fallback) that moves files, photos, videos, music and documents
between devices over local Wi-Fi **without internet or cloud dependency**. It
also ships built-in media viewers/players so recipients never need to leave the
app.

> **Compatibility:** Android 6.0 (API 23) → latest (target 35 by default; bump
> annually per Play policy). One shared Rust core, thin native UI shells per
> platform, zero cloud, zero telemetry.

---

## 1. Architecture

```
┌───────────────────────────────────────────────────┐
│              SHARED CORE ENGINE (Rust)             │
│  - QUIC transport (quinn) / TLS 1.3 (native)       │
│  - File chunking/reassembly, per-chunk QUIC stream │
│  - UDP multicast + mDNS discovery                  │
│  - Compiled to .so / .dll / .dylib / (WASM in dev) │
└──────────────┬────────────────────────────────────┘
               │  (Android: UniFFI/JNI · Desktop: direct)
   ┌───────────┼────────────┬──────────────┐
   ▼           ▼            ▼              ▼
 Android APK  Windows/Mac  Linux          Web
 (Kotlin)     (Tauri)      (Tauri)        (WebRTC)
```

**Non-negotiable rule:** the transfer engine and the media viewers are fully
decoupled. Media features are platform-native modules with zero dependency on
the Rust core beyond reading finished files.

---

## 2. Repository layout

| Path | What it is |
|------|------------|
| `rust-core/` | The shared QUIC/TLS-1.3 transfer engine (single source of truth) |
| `android/`   | Gradle multi-module Android app |
| `android/core-transfer/` | Builds the Rust `.so` (cargo-ndk) + UniFFI Kotlin bindings |
| `android/feature-*` | Image / video / music / doc viewers + MediaStore file browser |
| `pc-app/`    | Tauri desktop app (Windows/macOS/Linux) — same Rust core |
| `web/`       | WebRTC DataChannel browser fallback |
| `.github/workflows/` | CI that produces the APK/AAB and Windows MSI/EXE installer |

---

## 3. Building the real binaries

The sandbox this repo was authored in **does not have** the Rust toolchain,
Android SDK, JDK/Gradle, or Windows packaging tools installed, and the package
registries needed to fetch them are blocked. **Therefore the actual `.apk`,
`.aab`, `.msi` and `.exe` are produced by the GitHub Actions workflows** in
`.github/workflows/`, or by running the same commands on a machine that has the
toolchain.

### Android APK / AAB
```bash
# On a machine with Rust + Android NDK/SDK + JDK17 + Gradle 8.7
cargo install cargo-ndk
cargo install uniffi_bindgen --version 0.28.0
cd android
gradle assembleDebug      # -> app/build/outputs/apk/debug/app-debug.apk
gradle bundleRelease      # -> app/build/outputs/bundle/release/app-release.aab
```
Or just push to GitHub: `.github/workflows/android.yml` builds both and uploads
the artifacts.

### Windows installer (`.msi` + `.exe`)
```bash
# On a Windows machine with Rust + Node + WebView2
cd pc-app
npm install
npm run tauri build       # -> src-tauri/target/release/bundle/msi/*.msi
                          #    src-tauri/target/release/bundle/nsis/*.exe
```
Or push to GitHub: `.github/workflows/windows.yml` builds both installers and
uploads them.

### CLI (quick test of the engine)
```bash
cargo run --manifest-path rust-core/Cargo.toml -- serve --port 45843
cargo run --manifest-path rust-core/Cargo.toml -- send-files 192.168.1.20:45843 ./file.jpg
```

### Tests
```bash
cargo test --manifest-path rust-core/Cargo.toml          # unit + loopback transfer
```

---

## 4. Feature checklist

- **Transport:** QUIC via Rust `quinn`; TLS 1.3 built in; per-chunk QUIC streams;
  resume from last acknowledged byte (offset-addressed chunks).
- **Discovery:** UDP multicast + mDNS (core), BLE via platform native layer;
  permission logic branches by API level (23–30 → location; 31+ →
  BLUETOOTH_SCAN/CONNECT + NEARBY_WIFI_DEVICES; 33+ → POST_NOTIFICATIONS).
- **Android:** classic Views + RecyclerView; `MediaStore`/`ContentResolver`
  only (no raw File paths); `<queries>` block; foreground service
  `dataSync`; notification channel behind API 26 check.
- **File browser:** grouped by `DATE_ADDED` with date headers; two ViewHolder
  types; `recyclerview-selection` `SelectionTracker` for all selection.
- **Viewers:** image (Coil + pinch-zoom), video (Media3 + PiP + subtitles),
  music (Media3 `MediaSessionService` + lock-screen), docs (`PdfRenderer` +
  system/POI handoff).
- **Security:** TLS 1.3 everywhere, ephemeral self-signed certs, no persistent
  pairing, zero-cloud (no telemetry/analytics).

---

## 5. Status / known limits

- The Rust core and all shells are **source-complete** and wired for CI to
  compile. They have **not** been compiled in this authoring sandbox (no
  toolchain / no registry access), so the exact UniFFI-generated Kotlin API and
  the `gradle`/`tauri` build steps are exercised by the CI runners.
- To keep the Android build lean, `core-transfer` runs `cargo-ndk` +
  `uniffi-bindgen` to produce `morselink_core.kt` and the `.so`; set
  `MORSELINK_SKIP_NATIVE` to skip native on a dev box that lacks Rust.
- Android 17 (as referenced in the brief) is a future/preview API level; the
  project targets the latest stable (35) and is structured so `targetSdk` and
  `compileSdk` can be bumped annually (see `android/gradle.properties` and each
  module's `build.gradle`).
