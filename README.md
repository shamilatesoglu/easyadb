# easyadb

Pair an Android device for wireless ADB by scanning a QR code in your terminal.

`easyadb` shows a pairing QR, waits for the phone to scan it, then runs `adb pair` and `adb connect` and checks that the device is online.

## Requirements

- Android 11+ with Developer options and Wireless debugging
- Phone and computer on the same network
- [Platform-Tools](https://developer.android.com/tools/releases/platform-tools) with `adb` on `PATH` (or pass `--adb <path>` / set `EASYADB_ADB`)

## Install

```sh
cargo install easyadb
```

## Usage

```sh
easyadb
```

On the phone, go to **Developer options → Wireless debugging → Pair device with QR code** and scan the terminal. The QR expires after three minutes.

| Key | Action |
| --- | --- |
| `r` | New QR |
| `↑`/`↓`, `k`/`j` | Select a nearby device |
| `Enter` | Connect the selected (already paired) device |
| `?` | Help |
| `q`, `Ctrl+C` | Quit |

If mDNS discovery is blocked on your network, pair with a code instead:

```sh
easyadb pair 192.168.1.42:37123 --connect 192.168.1.42:40222
```

The pairing port is shown in the pairing-code dialog; the connection port is on the Wireless debugging page. They differ.

Other commands:

```sh
easyadb connect <host:port>
easyadb devices
easyadb doctor    # check adb, mDNS discovery and devices
easyadb --demo    # preview the UI without adb
```

## Notes

- Cryptography and host-key storage are handled by `adb`. Pairing secrets are passed over stdin and are not stored.
- Only the device that scanned the current QR is connected automatically.
- QR expiry does not revoke an existing pairing. Remove it on the phone under **Wireless debugging → Paired devices**.

## Android companion

`android/` contains an optional app that checks Wi-Fi and Developer options state, opens Wireless debugging settings, and adds a Quick Settings tile. It cannot pair on its own; Android does not allow that for regular apps.

```sh
cd android && ./gradlew :app:assembleDebug
```

Requires JDK 17 or 21 and Android SDK 36.

## License

MIT
