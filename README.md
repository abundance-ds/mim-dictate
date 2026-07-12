# Mim Dictate

Tiny desktop dictation app.

Mim records your voice, transcribes locally with Whisper, stores recent history, and pastes the text into the active app. Models are downloaded on demand and kept under Application Support.

## Requirements

- macOS 11+
- Node.js + npm
- Rust
- CMake
- Microphone permission.
- Keyboard access for auto-paste and the macOS Option hotkey.

## First Launch

Mim Dictate opens with a required setup checklist before the main app is shown:

1. Download the default Base speech model.
2. Allow microphone access.
3. Enable keyboard access.

Click each checklist item in the app. On macOS, use the in-app Keyboard access item first so macOS registers Mim Dictate before opening Privacy & Security manually. Keyboard access covers Accessibility for paste automation and Input Monitoring for the Option hotkey.

The default recording hotkey is:

- macOS: Option
- Windows/Linux: F8

After setup, the model and hotkey can be changed in the app settings.

## Troubleshooting

If recording works but text is not pasted into the current cursor position, keyboard access is the likely missing permission. Open Mim Dictate, click Keyboard access, then confirm Mim Dictate is enabled in macOS System Settings under:

- Privacy & Security -> Accessibility
- Privacy & Security -> Input Monitoring

If Mim Dictate is already listed there but paste still fails, toggle the permission off and on again.

## Dev

```sh
npm install
npm run dev
```

Use `npm run dev`, not `npm tauri dev`.

## Build

```sh
npm run build -- --no-bundle
```

Runtime data lives in:

```text
~/Library/Application Support/mim-dictate/
```
