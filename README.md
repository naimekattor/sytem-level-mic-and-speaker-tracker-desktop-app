# 🎙️ AI Desktop Assistant & Real-Time Meeting Transcriber

An ultra-fast, cross-stream desktop application built with **Tauri v2**, **Rust**, **React 19**, **Groq Whisper API**, and **Local Offline Whisper (`whisper.cpp`)**.

This application captures both **Microphone input** and **Speaker Loopback audio** (e.g., Microsoft Teams, Zoom, Google Meet calls, YouTube videos) simultaneously in real-time, performs Automatic Gain Control (AGC), Voice Activity Detection (VAD) with natural pause endpointing, and converts speech to text either via cloud APIs or **100% offline local AI models**.

---

## 📸 Overview & Key Features

- **Dual STT Engine Support**:
  - 💻 **Local Offline Engine**: Embedded `whisper-cli.exe` with `ggml-base.en.bin` model (~142 MB). 100% private, zero internet required, zero API key cost.
  - ☁️ **Groq Cloud Engine**: Ultra-fast `whisper-large-v3-turbo` for cloud-based transcriptions (<200ms latency).
- **Dual-Stream Audio Capture**: Captures microphone input and system speaker output (WASAPI Loopback on Windows) simultaneously.
- **Natural Utterance Endpointing (VAD)**:
  - Splits audio on **natural speech pauses (500ms–700ms)** instead of arbitrary time intervals.
  - Generates complete, unbroken sentences (`"I think we should invest in diversified index funds because they're lower risk."`).
  - 500ms overlap preroll buffer prevents word-boundary syllable clipping.
  - 15-second safety limit prevents memory leaks during continuous speech.
- **Automatic Gain Control (AGC) & Peak Normalization**:
  - Dynamically amplifies soft voices and whispers up to **20× loudness** without digital clipping.
- **Noise Gate & Hallucination Filter**:
  - Pre-AGC raw RMS noise gate (`0.0025`) discards ambient room fan/static hum.
  - Filters out common Whisper ghost phrases (`"Thanks for watching."`, `"Subtitles by..."`, `"you"`).
- **Single-Instance Mutex Process Safety**:
  - Employs `LOCAL_WHISPER_LOCK` (`try_lock()`) to guarantee single-instance C++ execution, keeping RAM under ~150 MB and preventing process crashes.
- **Real-Time Visual Audio Meters**: Low-latency RMS volume calculations displayed dynamically in the UI.
- **Export & Meeting Notes**: One-click copy for formatted conversation logs with timestamps and source labels.

---

## 📐 Architecture & System Design

```
                     ┌─────────────────────────────────────────┐
                     │          React 19 + TypeScript          │
                     │  (Visual Meters, Chat Feed, Engine Toggle)│
                     └────────────────────▲────────────────────┘
                                          │
                                Tauri IPC Events
                         ("audio-level", "transcription")
                                          │
                     ┌────────────────────┴────────────────────┐
                     │             Tauri v2 Core               │
                     │          (Rust Backend State)           │
                     └────────────────────▲────────────────────┘
                                          │
                     ┌────────────────────┴────────────────────┐
                     │          Audio Pipeline Thread          │
                     └───────▲────────────────────────▲────────┘
                             │                        │
       ┌─────────────────────┴──────┐          ┌──────┴─────────────────────┐
       │   Microphone Audio Stream  │          │  Speaker WASAPI Loopback   │
       │     (cpal::default_input)  │          │    (cpal::default_output)  │
       └────────────────────────────┘          └────────────────────────────┘
                             │                        │
                             └────────────┬───────────┘
                                          ▼
                             ┌────────────────────────┐
                             │  DSP & Resampling      │
                             │  • Stereo -> Mono      │
                             │  • RMS Vol Calc        │
                             │  • Resample to 16kHz   │
                             │  • Auto-Gain (AGC)     │
                             │  • Natural Pause VAD   │
                             └────────────┬───────────┘
                                          ▼
                             ┌────────────────────────┐
                             │  STT Engine Selector   │
                             └──────┬───────────┬─────┘
                                    │           │
                     ┌──────────────┴─┐       ┌─┴──────────────┐
                     │ Local Whisper  │       │ Groq Cloud API │
                     │ whisper-cli.exe│       │ Whisper Turbo  │
                     │ ggml-base.bin  │       └────────────────┘
                     └────────────────┘
```

---

## 🛠️ Detailed Step-by-Step Implementation Guide

### Phase 1: Project Setup & Dependencies
1. **Initialize Tauri v2 with Vite + React + TypeScript**:
   ```bash
   pnpm create tauri-app --template react-ts
   ```
2. **Rust Dependencies ([Cargo.toml](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src-tauri/Cargo.toml))**:
   - `cpal`: Cross-Platform Audio I/O library for microphone and WASAPI speaker loopback capture.
   - `reqwest`: HTTP client with `multipart` for Groq API integration.
   - `serde` & `serde_json`: JSON serialization for Tauri IPC.

---

### Phase 2: Native Audio Capture Engine ([lib.rs](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src-tauri/src/lib.rs))
1. **Microphone Stream**: Captures system mic input using `cpal::default_host().default_input_device()`.
2. **Speaker WASAPI Loopback (Windows)**: Captures pristine digital output from YouTube, Zoom, Teams, or Google Meet using `cpal::default_host().default_output_device()`.

---

### Phase 3: DSP, Automatic Gain Control (AGC) & Natural Pause VAD
1. **Stereo to Mono Conversion**:
   $$\text{Mono Sample} = \frac{\text{Left} + \text{Right}}{2}$$
2. **Audio Resampling**: Linear interpolation downsampling native 44.1kHz/48kHz sound card buffers to **16,000 Hz Mono PCM**.
3. **Automatic Gain Control (AGC)**:
   - Calculates raw peak level and applies dynamic gain boost up to **20×**:
     $$\text{Gain} = \min\left(\frac{0.90}{\text{Peak}}, 20.0\right)$$
   - Soft whispers are amplified to standard 0dB amplitude before Whisper decoding.
4. **Natural Utterance Pause Endpointing (VAD)**:
   - Tracks speech volume and silence counters.
   - Triggers utterance completion on **natural speech pauses (~500ms–700ms)** instead of fixed arbitrary timers.
   - Maintains a **500ms overlap preroll buffer (8,000 samples)** to prevent word-boundary clipping.
   - **Pre-AGC Noise Gate**: Rejects chunks with `raw_rms < 0.0025` to eliminate background room fan/static hum.

---

### Phase 4: Local Offline Whisper & Groq STT Engine

1. **Local Offline Engine (`whisper-cli.exe` + `ggml-base.en.bin`)**:
   - Resolves native executable from `src-tauri/bin/whisper-cli.exe` and model from `src-tauri/models/ggml-base.en.bin`.
   - Uses `LOCAL_WHISPER_LOCK` (`try_lock()`) to enforce single-instance process execution, capping RAM at ~150 MB and preventing parallel crashes.
   - Runs in ~150ms–300ms on CPU without needing Python or GPUs.

2. **Groq Cloud Engine**:
   - Posts 16kHz WAV multipart data to `https://api.groq.com/openai/v1/audio/transcriptions`.

---

### Phase 5: Tauri Inter-Process Communication (IPC) & React UI ([App.tsx](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src/App.tsx))
1. **Live Visual Meters**: High-frequency `audio-level` IPC events render real-time stereo volume bars.
2. **Transcription Stream**: Low-frequency `transcription` IPC events update chat history with source labels (`microphone` / `speaker`).
3. **Engine Toggle UI**: Switch seamlessly between **Groq Cloud** and **Local Offline Mode** in real-time.

---

## 📁 Directory Structure for Local Model & Executables

Place your offline binaries as follows:

```text
src-tauri/
├── bin/
│   ├── whisper-cli.exe
│   ├── whisper.dll
│   ├── ggml.dll
│   ├── ggml-base.dll
│   └── ggml-cpu-alderlake.dll  (and sibling CPU DLLs)
├── models/
│   └── ggml-base.en.bin         (142 MB model)
└── src/
    ├── main.rs
    └── lib.rs
```

> **Note**: Both `src-tauri/bin/` and `src-tauri/models/` are added to `.gitignore` to keep your Git repository lightweight.

---

## 🚀 How to Run Locally

### Prerequisites
1. **Node.js**: v18+ (using `pnpm` or `npm`)
2. **Rust**: Cargo & `rustc` 1.75+ ([rustup.rs](https://rustup.rs))
3. **C++ Build Tools**: Visual Studio Build Tools with C++ workload (Windows)

### Running Development App

1. **Install frontend dependencies**:
   ```bash
   pnpm install
   ```

2. **Run Tauri Dev Server**:
   ```bash
   pnpm tauri dev
   ```

3. **Select STT Engine**:
   - Choose **Local Offline Whisper** for instant 100% private transcription, or
   - Choose **Groq Cloud API** and enter your `gsk_...` key.

---

## 💡 Key Lessons & Performance Engineering Highlights

1. **Why `whisper-cli.exe` instead of Python?**
   `whisper.cpp` is written in lightweight C/C++. It requires **0 Python runtime, 0 PyTorch, 0 CUDA**, and runs on standard Windows CPUs with hardware SIMD/AVX2 acceleration.
2. **Why Natural Pause Endpointing over Fixed Chunking?**
   Fixed time chunking chops sentences mid-phrase (`"I think we should invest in"` | `"diversified index funds"`), ruining model context. Natural pause detection (~500ms pause) yields complete, coherent sentences.
3. **Single-Instance Mutex Safety**:
   Spawning multiple C++ processes in parallel for high-frequency mic & speaker streams exhausts system RAM (`147MB × N processes`). Enforcing `LOCAL_WHISPER_LOCK.try_lock()` keeps RAM footprint under ~150 MB with zero crashes.

---

## 📄 License

MIT License — Feel free to use, modify, and extend for your own projects!
