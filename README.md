# easyadb

**Less cable. More flow.** An Android setup companion and a Rust terminal client for QR-based wireless ADB pairing.

Run `easyadb` on your computer, open Android's **Wireless debugging → Pair device with QR code**, and scan your terminal. EasyADB discovers the pairing service, runs `adb pair`, finds the separate connection port, and checks that the phone is online in ADB.

## What you get

- A Ratatui dashboard with a high-contrast QR, session countdown, nearby devices, connected devices, and activity feed.
- Fresh, cryptographically random pairing credentials for each session; QR sessions expire after three minutes.
- Background discovery and pairing, bounded retries, subprocess timeouts, and terminal restoration on exit.
- Manual pairing, connection, and diagnostics commands for networks that block discovery.
- A native Kotlin Android app with Wi-Fi and Developer options checks, setup instructions, connection help, and a Quick Settings shortcut.

**Android limitation:** ordinary apps cannot enable wireless debugging or initiate system ADB pairing. The companion opens Developer options; **Android Settings performs the scan**. It does not request root, accessibility access, camera access, or privileged debugging permissions. The Rust client also works without installing the companion.

## Requirements

- Android 11 or newer, with Developer options and Wireless debugging available. Some manufacturers or managed profiles restrict these features.
- Phone and computer on the same Wi-Fi network, with peer-to-peer traffic permitted.
- Recent [Android SDK Platform-Tools](https://developer.android.com/tools/releases/platform-tools), with `adb` on your PATH.
- Rust 1.88 or newer to build the terminal client.
- JDK 17 or 21 and Android SDK 36 to build the Android app. Android Studio is optional.

## Run the terminal client

```sh
cargo run --release
```

Or install it locally:

```sh
cargo install --path crates/easyadb --locked
easyadb
```

If ADB is elsewhere:

```sh
easyadb --adb /path/to/android-sdk/platform-tools/adb
# Alternatively, set EASYADB_ADB to the executable path.
```

1. On your phone, enable Developer options by tapping **Build number** seven times in **About phone** (the location varies by manufacturer).
2. In Developer options, turn on **Wireless debugging** and allow your current network.
3. Select **Pair device with QR code** and scan the QR in the terminal. The regular camera app cannot initiate this pairing.
4. Wait for **Connected · ready to develop**. The device is now available to normal ADB, Android Studio, and other development tools.

Use a terminal with true color and Unicode half-block support. EasyADB displays the required minimum terminal size if the QR would be clipped; reducing font size usually helps.

| Key | Action |
| --- | --- |
| `r` | Generate a new QR; scan it to pair another phone |
| `↑` / `↓` or `k` / `j` | Select a nearby connection service |
| `Enter` | Connect the selected, already-paired device |
| `?` | Show connection help |
| `q` / `Ctrl+C` | Quit; existing ADB connections remain available |

`easyadb --demo` previews the dashboard without invoking ADB. The sample device in that view is explicitly marked as a demo.

## Build the Android companion

Open the `android/` directory in Android Studio, or build from the command line:

```sh
export ANDROID_HOME=/path/to/android-sdk
# JAVA_HOME must point to a JDK 17 or 21 installation.
cd android
./gradlew :app:assembleDebug
```

The APK is written to `android/app/build/outputs/apk/debug/app-debug.apk`.

Install it through Android Studio, transfer the APK to your phone, or use an existing ADB connection:

```sh
adb -s DEVICE_SERIAL install -r android/app/build/outputs/apk/debug/app-debug.apk
```

The app's **Open Developer options** button takes you to system settings. Find **Wireless debugging**, then choose the QR scanner. The app reports Wi-Fi and Developer options state; the **terminal** reports pairing and ADB connection state. It does not infer a connection from merely having Wi-Fi enabled.

Use **Add Quick Settings shortcut** for faster access next time. Android 13+ offers a system add-tile prompt; on Android 11–12, add the EasyADB tile manually using the Quick Settings editor.

## Manual pairing and diagnostics

```sh
easyadb doctor
easyadb devices
```

If mDNS discovery is blocked, choose **Pair device with pairing code** on your phone:

```sh
easyadb pair 192.168.1.42:37123
# Enter the six-digit code at the hidden prompt.

easyadb connect 192.168.1.42:40222
```

The **pairing port** comes from the pairing-code dialog. The **connection port** comes from the main Wireless debugging page. They are different and can change whenever wireless debugging restarts. You can provide both at once:

```sh
easyadb pair 192.168.1.42:37123 --connect 192.168.1.42:40222
```

IPv6 socket addresses use brackets, such as `'[fe80::1234%4]:37123'` (a numeric interface scope ID may be needed).

If discovery stays empty:

- Open the system QR scanner; the pairing service appears after a scan.
- Check both devices are on the same network. Guest Wi-Fi, client isolation, VPN routing, and firewall rules may block multicast or device-to-device traffic.
- Run `easyadb doctor` and update Platform-Tools if `adb pair` or `adb mdns` is unavailable.
- Rescan a fresh QR after a failure or use manual pairing. A successful `adb mdns check` only reports the backend; it does not prove multicast crosses your network.

## Development

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cd android
./gradlew :app:assembleDebug :app:lintDebug
```

Tests exercise the QR wire format and quiet zone, mDNS and device parsing, process timeouts and secret redaction, terminal sizing and expiry, plus a simulated scan → pair → connect → online sequence. The simulation includes unrelated nearby devices, different pairing/connection ports, and ADB errors that still return exit code zero.

Hardware acceptance still requires a physical Android phone: scan the terminal QR, verify `adb devices -l`, toggle wireless debugging to check reconnection, and forget the host in Settings to check re-pairing. An emulator cannot validate the real camera and Wi-Fi multicast path.

## Architecture and trust

```text
Android companion ── opens ──→ Developer options
                                     │
Rust TUI ── displays QR ──→ Android system QR scanner
   │                                 │
   │ ←──── mDNS pairing service ──────┘
   ├── adb pair (secret over stdin)
   ├── adb connect (separate TLS connection service)
   └── adb devices -l (verify online)
```

EasyADB delegates cryptography and host-key storage to official ADB. It only automatically pairs the exact mDNS instance named in the current QR, and automatically connects services at that paired phone's IP. Other nearby devices require an explicit selection. Pairing secrets are not placed in process arguments or saved by EasyADB; command output is redacted if ADB echoes a secret. There are no analytics or external application servers.

The three-minute expiry stops this TUI from starting new pairing attempts with that QR; it does **not** revoke an already-paired computer. Android remembers the ADB host key. Remove that trust using **Wireless debugging → Paired devices → Forget**. A pairing operation already in flight may finish at the expiry boundary.

The protocol follows [AOSP's ADB Wi-Fi architecture](https://android.googlesource.com/platform/packages/modules/adb/+/HEAD/docs/dev/adb_wifi.md). System pairing is guarded by `MANAGE_DEBUGGING` in [AdbService](https://android.googlesource.com/platform/frameworks/base/+/master/services/core/java/com/android/server/adb/AdbService.java).

## Project layout

```text
crates/easyadb/src/   Rust CLI, TUI, ADB adapter, pairing session, worker
crates/easyadb/tests/ Simulated ADB protocol fixture
android/app/         Native Kotlin companion and Quick Settings tile
```

MIT licensed. See [LICENSE](LICENSE).
