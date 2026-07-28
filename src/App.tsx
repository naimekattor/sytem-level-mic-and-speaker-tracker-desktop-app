import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface AudioLevelPayload {
  source: "microphone" | "speaker";
  volume: number;
  samples_count: number;
}

interface TranscriptionPayload {
  source: "microphone" | "speaker";
  text: string;
}

interface ChatMessage {
  id: string;
  source: "microphone" | "speaker";
  text: string;
  timestamp: string;
}

function App() {
  const [isRecording, setIsRecording] = useState<boolean>(true);
  const [micVol, setMicVol] = useState<number>(0);
  const [speakerVol, setSpeakerVol] = useState<number>(0);
  const [apiKey, setApiKey] = useState<string>("");
  const [isKeySaved, setIsKeySaved] = useState<boolean>(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);

  const [sttEngine, setSttEngine] = useState<"groq" | "local">("groq");

  useEffect(() => {
    // 1. Fetch initial status
    invoke<boolean>("get_audio_status").then(setIsRecording);

    // 2. Load saved Groq API key & STT engine preference from localStorage
    const savedKey = localStorage.getItem("groq_api_key");
    if (savedKey) {
      setApiKey(savedKey);
      invoke("set_groq_api_key", { apiKey: savedKey })
        .then(() => setIsKeySaved(true))
        .catch(console.error);
    }

    const savedEngine = localStorage.getItem("stt_engine") as "groq" | "local" | null;
    if (savedEngine) {
      setSttEngine(savedEngine);
      invoke("set_stt_engine", { engine: savedEngine }).catch(console.error);
    }

    // 3. Listen for audio-level events
    const unlistenLevel = listen<AudioLevelPayload>("audio-level", (event) => {
      const { source, volume } = event.payload;
      if (source === "microphone") setMicVol(volume);
      else if (source === "speaker") setSpeakerVol(volume);
    });

    // 4. Listen for transcription events
    const unlistenTranscribe = listen<TranscriptionPayload>("transcription", (event) => {
      const { source, text } = event.payload;
      const newMsg: ChatMessage = {
        id: Math.random().toString(),
        source,
        text,
        timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }),
      };
      setMessages((prev) => [newMsg, ...prev].slice(0, 50));
    });

    return () => {
      unlistenLevel.then((u) => u());
      unlistenTranscribe.then((u) => u());
    };
  }, []);

  const handleSaveApiKey = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!apiKey.trim()) return;
    await invoke("set_groq_api_key", { apiKey: apiKey.trim() });
    localStorage.setItem("groq_api_key", apiKey.trim());
    setIsKeySaved(true);
  };

  const handleEngineSwitch = async (engine: "groq" | "local") => {
    setSttEngine(engine);
    await invoke("set_stt_engine", { engine });
    localStorage.setItem("stt_engine", engine);
  };

  const toggleRecording = async () => {
    if (isRecording) {
      await invoke("stop_audio_capture");
      setIsRecording(false);
      setMicVol(0);
      setSpeakerVol(0);
    } else {
      await invoke("start_audio_capture");
      setIsRecording(true);
    }
  };

  return (
    <div className="dashboard">
      <header className="header">
        <h1>🎙️ AI Assistant Real-Time Transcriber</h1>
        <p className="subtitle">Groq Whisper Cloud STT + Local Whisper Model Engine</p>
      </header>

      {/* Engine Switcher Mode Card */}
      <div className="engine-card">
        <h3>⚙️ Select Speech-to-Text Engine:</h3>
        <div className="engine-toggle-buttons">
          <button
            className={`engine-btn ${sttEngine === "groq" ? "active" : ""}`}
            onClick={() => handleEngineSwitch("groq")}
          >
            ⚡ Groq Cloud Whisper API (Ultra-Fast &lt;200ms)
          </button>
          <button
            className={`engine-btn ${sttEngine === "local" ? "active" : ""}`}
            onClick={() => handleEngineSwitch("local")}
          >
            💻 Local Offline Whisper (ggml-base.en.bin 142MB)
          </button>
        </div>
      </div>

      {/* Groq API Key Input Form (Only visible in Groq mode) */}
      {sttEngine === "groq" && (
        <div className="api-card">
          <form onSubmit={handleSaveApiKey} className="api-form">
            <label htmlFor="api-key-input"><strong>🔑 Groq API Key:</strong></label>
            <input
              id="api-key-input"
              type="password"
              placeholder="Paste gsk_... key from console.groq.com"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
            />
            <button type="submit" className="save-btn">
              {isKeySaved ? "✓ Key Active" : "Set API Key"}
            </button>
          </form>
          {!isKeySaved && (
            <p className="hint">
              💡 Get a free API key at <a href="https://console.groq.com" target="_blank" rel="noreferrer">console.groq.com</a> for ultra-fast (&lt;200ms) transcriptions!
            </p>
          )}
        </div>
      )}

      <div className="status-card">
        <div className="status-info">
          <span className={`status-badge ${isRecording ? "active" : "paused"}`}>
            {isRecording ? "🔴 RECORDING LIVE" : "⏸️ CAPTURE PAUSED"}
          </span>
        </div>
        <button
          className={`control-btn ${isRecording ? "stop" : "start"}`}
          onClick={toggleRecording}
        >
          {isRecording ? "Stop Capture" : "Start Capture"}
        </button>
      </div>

      <div className="streams-grid">
        {/* Microphone Meter */}
        <div className="stream-card mic">
          <div className="card-header">
            <span className="icon">🎤</span>
            <h3>Microphone Stream</h3>
          </div>
          <div className="meter-container">
            <div
              className="meter-bar mic-bar"
              style={{ width: `${Math.min(micVol * 300, 100)}%` }}
            />
          </div>
          <div className="card-stats">
            <div>Volume RMS: <strong>{(micVol * 100).toFixed(1)}%</strong></div>
          </div>
        </div>

        {/* Speaker Loopback Meter */}
        <div className="stream-card speaker">
          <div className="card-header">
            <span className="icon">🔊</span>
            <h3>Speaker Loopback</h3>
          </div>
          <div className="meter-container">
            <div
              className="meter-bar speaker-bar"
              style={{ width: `${Math.min(speakerVol * 300, 100)}%` }}
            />
          </div>
          <div className="card-stats">
            <div>Volume RMS: <strong>{(speakerVol * 100).toFixed(1)}%</strong></div>
          </div>
        </div>
      </div>

      {/* Transcription Conversation Feed */}
      <div className="transcription-feed">
        <div className="feed-header">
          <h2>💬 Live Meeting Transcripts (Teams / Zoom)</h2>
          <div className="feed-actions">
            {messages.length > 0 && (
              <>
                <button
                  className="action-btn copy"
                  onClick={() => {
                    const text = messages
                      .slice()
                      .reverse()
                      .map((m) => `[${m.timestamp}] ${m.source === "microphone" ? "You" : "Teams/Zoom Speaker"}: ${m.text}`)
                      .join("\n");
                    navigator.clipboard.writeText(text);
                    alert("📋 Full meeting transcript copied to clipboard!");
                  }}
                >
                  📋 Copy Notes
                </button>
                <button className="action-btn clear" onClick={() => setMessages([])}>
                  🗑️ Clear
                </button>
              </>
            )}
          </div>
        </div>

        <div className="messages-list">
          {messages.length === 0 ? (
            <div className="empty-state">
              <p>👥 Start your Teams, Zoom, or Google Meet call! Your voice and meeting participants will be transcribed live here...</p>
            </div>
          ) : (
            messages.map((msg) => (
              <div key={msg.id} className={`chat-bubble ${msg.source}`}>
                <div className="bubble-header">
                  <span>{msg.source === "microphone" ? "🎤 You (Microphone)" : "👥 Teams / Zoom Participant"}</span>
                  <span className="timestamp">{msg.timestamp}</span>
                </div>
                <div className="bubble-text">{msg.text}</div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
