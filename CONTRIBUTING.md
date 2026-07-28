# 🤝 Contributing to AI Desktop Assistant & Real-Time Transcriber

Thank you for your interest in contributing! Contributions from the open-source community help make this real-time audio copilot better for everyone.

---

## 🛠️ How to Contribute

### 1. Fork & Clone the Repository

1. **Fork** the repository on GitHub: [`https://github.com/naimekattor/sytem-level-mic-and-speaker-tracker-desktop-app`](https://github.com/naimekattor/sytem-level-mic-and-speaker-tracker-desktop-app)
2. **Clone** your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/sytem-level-mic-and-speaker-tracker-desktop-app.git
   cd sytem-level-mic-and-speaker-tracker-desktop-app
   ```

---

### 2. Set Up Development Environment

1. **Install Frontend Dependencies**:
   ```bash
   pnpm install
   # or: npm install
   ```

2. **Ensure System Prerequisites**:
   - **Node.js**: v18 or higher
   - **Rust**: `cargo` & `rustc` 1.75+ ([rustup.rs](https://rustup.rs))
   - **C++ Build Tools**: Visual Studio Build Tools with C++ workload (for Windows native binaries)

3. **Download Local Binaries (Optional for Offline Testing)**:
   - Place `whisper-cli.exe` and sibling DLLs in `src-tauri/bin/`
   - Place `ggml-base.en.bin` in `src-tauri/models/`
   > Note: `bin/` and `models/` are listed in `.gitignore` to keep git commits lightweight.

4. **Launch Tauri Dev Server**:
   ```bash
   pnpm tauri dev
   ```

---

### 3. Create a Feature Branch

Create a branch named after the feature or bugfix you are working on:
```bash
git checkout -b feature/your-feature-name
# or: git checkout -b fix/issue-description
```

---

### 4. Guidelines & Code Style

- **Rust Backend (`src-tauri/src/lib.rs`)**:
  - Keep audio callbacks fast and non-blocking.
  - Test Rust build cleanly with `cargo check` inside `src-tauri/`.
  - Maintain thread safety using `Arc<Mutex<T>>` or `AtomicBool`.
- **React Frontend (`src/App.tsx`, `src/components/`)**:
  - Keep UI responsive and clean.
  - Format with Prettier / ESLint.

---

### 5. Submit a Pull Request (PR)

1. Commit your changes:
   ```bash
   git add .
   git commit -m "feat: add feature description"
   ```
2. Push to your fork:
   ```bash
   git push origin feature/your-feature-name
   ```
3. Open a **Pull Request** on GitHub against the `main` branch. Provide a concise summary of your changes and any testing done.

---

## 💡 Ideas for Contributions

- 🚀 **AI Copilot Real-Time Answer Generator**: Expand Groq/LLaMA response suggestions for live meeting questions.
- 👤 **Multi-Speaker Diarization**: Identify multiple remote speakers (*Speaker 1*, *Speaker 2*) on virtual calls.
- 🌐 **Browser Extension Companion**: Extension popup / sidepanel to capture browser tab audio directly.
- 🎨 **UI Modernization**: Add custom themes, visual waveform analyzers, or floating overlay modes.

---

## 📜 Code of Conduct

Be kind, respectful, and collaborative. We welcome developers of all skill levels!
