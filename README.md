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

## Local speech models

Mim already runs **whisper.cpp**, through `whisper-rs`, with Metal GPU acceleration on macOS. No separate C++ app or Python service is needed.

The model picker offers these multilingual models (including German and English). Downloads are separate from the app bundle:

| Model | Download |
| --- | ---: |
| Tiny | 78 MB |
| Base (default) | 148 MB |
| Small | 488 MB |
| Tiny Q5 | 32 MB |
| Base Q5 | 60 MB |
| Small Q5 | 190 MB |
| Large v3 Turbo Q5 | 574 MB |

Sizes use decimal MB from the [whisper.cpp model repository](https://huggingface.co/ggerganov/whisper.cpp/tree/main). Large v3 Turbo Q5 is about **18% larger than Small**, but nearly **4× Base**. The upstream model list expresses it as [547 MiB](https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md).

**Q5** means the model weights use roughly 5-bit quantization, reducing disk and memory use with a possible accuracy tradeoff. It can improve speed depending on the hardware; smaller files do not guarantee faster dictation. See [whisper.cpp quantization](https://github.com/ggml-org/whisper.cpp#quantization).

**Turbo** is a version of Large v3 with fewer decoder layers, designed for faster transcription with some accuracy loss versus full Large v3. Try **Large v3 Turbo Q5** for a quality upgrade from Base/Small, or **Small Q5** for a compact option. Actual accuracy, latency, and memory use depend on your recordings and Mac; these are not local benchmark results. See the [Turbo model card](https://huggingface.co/openai/whisper-large-v3-turbo).

Open Settings → Model, select a model, then download it when prompted. Base remains the first-launch default, and existing selections are preserved. Switching models releases the previous model before loading the next one.

## Troubleshooting

Click the menu bar icon to toggle the panel, or the Dock icon to show it. Closing the panel hides it; click **Quit** in the panel header to exit, including during setup. You can also right-click the menu bar icon and choose Quit. The panel uses the current tray/display position with a fallback when displays change. Whisper's GPU resources are released before quitting or restarting.

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

Backend checks:

```sh
cd src-tauri
cargo test --locked --lib
cargo fmt --check
```

To check native model loading, inference, and GPU cleanup in a separate process (using an already downloaded model):

```sh
MIM_TEST_APP_DIR="$HOME/Library/Application Support/mim-dictate" \
MIM_TEST_MODEL=base \
cargo test --locked --lib native_model_transcribes_and_shuts_down -- --ignored
```

Runtime data lives in:

```text
~/Library/Application Support/mim-dictate/
```
