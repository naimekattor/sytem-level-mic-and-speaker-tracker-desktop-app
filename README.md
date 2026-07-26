# 🎙️ AI Desktop Assistant & Real-Time Meeting Transcriber

An ultra-fast, cross-stream desktop application built with **Tauri v2**, **Rust**, **React 19**, and **Groq Whisper API**.

This application captures both **Microphone input** and **Speaker Loopback audio** (e.g. Microsoft Teams, Zoom, Google Meet calls) simultaneously in real-time, performs digital signal processing (DSP) and voice activity detection (VAD), and converts speech to text with sub-200ms latency.

---

## 📸 Overview & Features

- **Dual-Stream Audio Capture**: Captures your microphone and speaker output (WASAPI Loopback on Windows) simultaneously.
- **Real-Time Visual Audio Meters**: Low-latency RMS volume calculations displayed dynamically in the UI.
- **Ultra-Fast Speech-to-Text**: Powered by Groq's `whisper-large-v3-turbo` cloud engine (<200ms transcription latency).
- **VAD & Smart Speech Accumulator**: Detects voice activity and automatically chunks continuous speech into 16kHz WAV buffers.
- **Rate-Limit & Error Handling**: Implements cooldown timers and fallback handling for Groq API free-tier limits (20 RPM).
- **Export & Meeting Notes**: Easily copy full formatted conversation logs with timestamp and source labels to clipboard.

---

## 📐 Architecture & System Design

```
                     ┌─────────────────────────────────────────┐
                     │          React 19 + TypeScript          │
                     │  (Visual Meters, Chat Feed, API Form)   │
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
                             │  • VAD & Silence Counter│
                             └────────────┬───────────┘
                                          ▼
                             ┌────────────────────────┐
                             │ WAV Binary Encoder &   │
                             │ Groq STT HTTP Request  │
                             └────────────────────────┘
```

---

## 🛠️ Detailed Step-by-Step Implementation Guide

If you want to understand how every part of this system was constructed or replicate it from scratch, follow these step-by-step phases:

### Phase 1: Project Setup & Dependencies
1. **Initialize Tauri v2 with Vite + React + TypeScript**:
   ```bash
   pnpm create tauri-app --template react-ts
   ```
2. **Configure Rust Dependencies ([Cargo.toml](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src-tauri/Cargo.toml))**:
   - `cpal`: Cross-Platform Audio I/O library for audio device selection and loopback stream capture.
   - `reqwest`: HTTP client with `multipart` and `blocking` features for uploading WAV data to Groq.
   - `serde` & `serde_json`: JSON serialization for Tauri IPC state management.
   - `tokio`: Async runtime management.

---

### Phase 2: Native Audio Capture Engine ([lib.rs](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src-tauri/src/lib.rs))
1. **Microphone Stream**:
   - Enumerate input devices using `cpal::default_host().default_input_device()`.
   - Build an input stream using `build_input_stream` with callback closure sending audio sample buffers into a MPSC channel (`std::sync::mpsc::channel`).
2. **Speaker WASAPI Loopback Capture (Windows)**:
   - Enumerate output device (`cpal::default_host().default_output_device()`).
   - Create a loopback input stream on the default speaker device so that played audio from Teams/Zoom/YouTube is captured directly from system audio outputs.

---

### Phase 3: Digital Signal Processing (DSP) & VAD Engine
1. **Channel Normalization (Stereo to Mono)**:
   - Convert 2-channel audio (Left + Right) to 1-channel mono by averaging sample pairs:
     $$\text{Mono Sample} = \frac{\text{Left} + \text{Right}}{2}$$
2. **RMS Volume Calculation**:
   - Compute Root Mean Square (RMS) to track live audio levels:
     $$\text{RMS} = \sqrt{\frac{1}{N} \sum_{i=1}^{N} x_i^2}$$
3. **Audio Resampling**:
   - Native sound cards record at 44,100 Hz or 48,000 Hz. Speech-to-text models (Whisper) require **16,000 Hz mono PCM**.
   - Implemented linear interpolation resampling in `resample_to_16k()` to dynamically downsample sample buffers to 16 kHz.
4. **Voice Activity Detection (VAD) & Accumulator**:
   - Silence thresholding ($> 0.010$ RMS for mic, $> 0.012$ RMS for speaker).
   - Accumulate samples in a thread-safe buffer until either ~2.5 seconds of continuous speech or 1.5 seconds of silence after speech is reached.

---

### Phase 4: In-Memory WAV Encoder & Groq STT Integration
1. **Raw PCM to WAV Conversion**:
   - Function `create_wav_bytes()` manually constructs a valid 44-byte RIFF/WAVE header (RIFF header, `fmt ` chunk, `data` chunk) and converts 32-bit float samples to 16-bit signed PCM (`i16`).
2. **Groq API Client**:
   - Post multipart form data containing model `whisper-large-v3-turbo` and raw audio byte buffer to `https://api.groq.com/openai/v1/audio/transcriptions`.
3. **Rate Limiting & Debugging**:
   - Free tier limit is 20 RPM (Requests Per Minute). Enforced a 3.0-second cooldown per audio stream.
   - Export debug files to `debug_audio/last_mic_speech.wav` and `debug_audio/last_speaker_speech.wav` to inspect audio quality on disk.

---

### Phase 5: Tauri Inter-Process Communication (IPC) & React UI ([App.tsx](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src/App.tsx))
1. **IPC Event Emitter**:
   - High-frequency event (`audio-level`) sent from Rust background thread to React for real-time visual meter rendering.
   - Low-frequency event (`transcription`) emitted when STT completes, updating chat history.
2. **Frontend UI State**:
   - Groq API key input form with local storage persistence.
   - Start / Pause recording toggles via Tauri commands (`invoke("start_audio_capture")`).
   - Copy notes functionality for instant meeting transcript exporting.

---

## 🚀 How to Run Locally

### Prerequisites
1. **Node.js**: v18 or higher (recommend `pnpm` or `npm`)
2. **Rust**: Cargo & `rustc` 1.75+ (install via [rustup.rs](https://rustup.rs))
3. **C++ Build Tools**: Visual Studio Build Tools with C++ workload (for Windows)
4. **Groq API Key**: Free API key from [console.groq.com](https://console.groq.com)

### Installation Steps

1. **Clone the repository**:
   ```bash
   git clone https://github.com/your-username/ai_desktop_assistant.git
   cd ai_desktop_assistant
   ```

2. **Install frontend dependencies**:
   ```bash
   pnpm install
   # or: npm install
   ```

3. **Run the Tauri Dev App**:
   ```bash
   pnpm tauri dev
   # or: npm run tauri dev
   ```

4. **Add your Groq API Key**:
   - Once the application window opens, paste your `gsk_...` key in the top form and click **Set API Key**.

---

## 💡 How You Can Improve & Extend This Project

If you want to contribute, learn deeper, or take this project to a production level, here are key areas for improvement:

### 1. ⚡ Ultra-Low Latency AI Copilot: Real-Time Answer Suggestion for Remote Speakers
- **Concept**: As soon as a remote participant (on Teams, Zoom, or Google Meet) finishes asking a question or speaking, the system automatically generates and displays a suggested reply/answer for YOU on screen in **< 300ms total latency**.

#### 🏎️ Latency Breakdown (< 300ms End-to-End)
| Pipeline Stage | Technology Used | Latency |
| :--- | :--- | :--- |
| **1. Audio Capture & VAD** | `cpal` WASAPI loopback + silence detector | ~20ms |
| **2. Speech-to-Text (STT)** | Groq `whisper-large-v3-turbo` | ~120ms - 150ms |
| **3. LLM Response Generation** | Groq `llama-3.1-8b-instant` (800+ tok/s) | ~80ms - 120ms |
| **4. IPC & UI Rendering** | Tauri Rust `emit()` -> React 19 UI | ~2ms |
| **Total End-to-End** | **Audio to Suggested Answer on Screen** | **~250ms – 300ms** 🚀 |

#### 🛠️ How to Implement (Rust Backend Snippet)

When `AudioSource::Speaker` is transcribed in [lib.rs](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src-tauri/src/lib.rs), spawn an asynchronous non-blocking thread to request a fast response suggestion from Groq LLaMA 3.1:

```rust
// In lib.rs: Trigger LLM suggestion when speaker speech is transcribed
pub fn suggest_answer_groq(speaker_text: &str, api_key: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    let payload = serde_json::json!({
        "model": "llama-3.1-8b-instant",
        "messages": [
            {
                "role": "system",
                "content": "You are a real-time meeting AI co-pilot. Analyze what the remote speaker just said and suggest a concise, smart answer (1-2 bullet points or 1 short sentence) for the user to reply with."
            },
            {
                "role": "user",
                "content": format!("Remote Speaker said: \"{}\"", speaker_text)
            }
        ],
        "temperature": 0.3,
        "max_tokens": 100
    });

    let response = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = response.json().map_err(|e| e.to_string())?;
    Ok(json["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string())
}
```

#### 🎨 React Frontend Integration ([App.tsx](file:///c:/Users/naim%20dev/Desktop/learning_tauri_rust/ai_desktop_assistant/src/App.tsx))
Listen for the custom `ai-suggestion` event and display a glowing suggestion badge attached to the speaker message bubble:

```tsx
// Listen for ultra-fast AI suggestion events
listen<{ speaker_text: string; suggested_reply: string; latency_ms: number }>(
  "ai-suggestion",
  (event) => {
    const { speaker_text, suggested_reply, latency_ms } = event.payload;
    setMessages((prev) =>
      prev.map((msg) =>
        msg.source === "speaker" && msg.text === speaker_text
          ? { ...msg, suggestion: suggested_reply, latency: latency_ms }
          : msg
      )
    );
  }
);
```

---

### 2. 👤 Speaker Diarization (Multi-Speaker Identification)
- **Current state**: Distinguishes between Microphone (You) vs. Speaker (Teams/Zoom call).
- **Improvement**: Integrate PyAnnote or ONNX speaker recognition models to distinguish between *Speaker 1*, *Speaker 2*, and *Speaker 3* in virtual meetings.

### 3. 🔒 Offline / On-Device Speech Recognition
- **Current state**: Relies on Groq cloud API.
- **Improvement**: Add an offline option using `whisper.cpp` bindings in Rust (`whisper-rs`) or ONNX runtime with quantized Whisper models for total privacy and zero network dependency.

### 4. 🎛️ Advanced VAD & Noise Suppression
- **Current state**: Basic RMS volume thresholding.
- **Improvement**: Integrate **Silero VAD** or **RNNoise / DeepFilterNet** for background noise reduction (keyboard clicks, fan noise, room echo) before sending to STT.

### 5. 🪟 Floating Overlay & PIP UI
- **Improvement**: Create an always-on-top transparent widget or system tray icon with global hotkeys (e.g. `Ctrl + Shift + T`) to show live transcripts in a compact overlay during video calls.

---

## 📄 License

Distributed under the MIT License. See `LICENSE` for details.

