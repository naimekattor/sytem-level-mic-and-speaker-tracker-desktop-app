use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::Emitter;

#[derive(Clone, Serialize)]
pub struct AudioLevelPayload {
    pub source: String,
    pub volume: f32,
    pub samples_count: usize,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionPayload {
    pub source: String,
    pub text: String,
}

pub struct AppState {
    pub is_recording: Arc<AtomicBool>,
    pub groq_api_key: Arc<Mutex<String>>,
    pub stt_engine: Arc<Mutex<String>>, // "groq" or "local"
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn start_audio_capture(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state.is_recording.store(true, Ordering::SeqCst);
    println!("▶️ Audio capture started via Tauri command");
    Ok("Audio capture started".into())
}

#[tauri::command]
fn stop_audio_capture(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state.is_recording.store(false, Ordering::SeqCst);
    println!("⏸️ Audio capture stopped via Tauri command");
    Ok("Audio capture stopped".into())
}

#[tauri::command]
fn get_audio_status(state: tauri::State<'_, AppState>) -> bool {
    state.is_recording.load(Ordering::SeqCst)
}

#[tauri::command]
fn set_groq_api_key(api_key: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut key_lock = state.groq_api_key.lock().map_err(|e| e.to_string())?;
    *key_lock = api_key.trim().to_string();
    println!("🔑 Groq API Key updated successfully");
    Ok("Groq API Key set".into())
}

#[tauri::command]
fn set_stt_engine(engine: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut eng_lock = state.stt_engine.lock().map_err(|e| e.to_string())?;
    *eng_lock = engine.trim().to_lowercase();
    println!("⚙️ STT Engine set to: {}", eng_lock);
    Ok(format!("STT Engine set to {}", eng_lock))
}

pub enum AudioSource {
    Microphone,
    Speaker,
}

pub struct AudioPacket {
    pub source: AudioSource,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub fn get_default_microphone() -> Option<cpal::Device> {
    let host = cpal::default_host();
    let mic = host.default_input_device();

    match &mic {
        Some(device) => {
            let name = device.name().unwrap_or_else(|_| "Unknown Microphone".to_string());
            println!("✅ Selected Microphone: {}", name);
        }
        None => {
            println!("❌ No default microphone found on Windows.");
        }
    }

    mic
}

pub fn print_microphone_config(device: &cpal::Device) {
    match device.default_input_config() {
        Ok(config) => {
            println!("\n--- Microphone Configuration ---");
            println!("Sample Rate: {} Hz", config.sample_rate().0);
            println!("Channels: {}", config.channels());
            println!("Sample Format: {:?}", config.sample_format());
        }
        Err(e) => {
            println!("❌ Failed to get default input configuration: {}", e);
        }
    }
}

pub fn build_microphone_stream(device: &cpal::Device, tx: Sender<AudioPacket>) -> Option<cpal::Stream> {
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            println!("❌ Failed to get input config: {}", e);
            return None;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    println!("\n--- Building Microphone Input Stream ({} Hz, {} channels) ---", sample_rate, channels);
    let err_fn = |err| eprintln!("❌ Microphone stream error: {}", err);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let packet = AudioPacket {
                    source: AudioSource::Microphone,
                    samples: data.to_vec(),
                    sample_rate,
                    channels,
                };
                let _ = tx.send(packet);
            },
            err_fn,
            None,
        ),
        sample_format => {
            println!("❌ Unsupported sample format for mic: {:?}", sample_format);
            return None;
        }
    };

    match stream {
        Ok(stream) => {
            println!("✅ Microphone input stream built successfully.");
            Some(stream)
        }
        Err(err) => {
            println!("❌ Error building microphone input stream: {}", err);
            None
        }
    }
}

pub fn get_default_speaker() -> Option<cpal::Device> {
    let host = cpal::default_host();
    let speaker = host.default_output_device();
    if let Some(ref dev) = speaker {
        let name = dev.name().unwrap_or_else(|_| "Unknown Speaker".to_string());
        println!("✅ Selected Speaker: {}", name);
    } else {
        println!("❌ No default speaker found on Windows.");
    }
    speaker
}

pub fn build_speaker_loopback_stream(device: &cpal::Device, tx: Sender<AudioPacket>) -> Option<cpal::Stream> {
    let config = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            println!("❌ Failed to get speaker output config: {}", e);
            return None;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    println!("\n--- Building Speaker WASAPI Loopback Stream ({} Hz, {} channels) ---", sample_rate, channels);
    let err_fn = |err| eprintln!("❌ Speaker loopback stream error: {}", err);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let packet = AudioPacket {
                    source: AudioSource::Speaker,
                    samples: data.to_vec(),
                    sample_rate,
                    channels,
                };
                let _ = tx.send(packet);
            },
            err_fn,
            None,
        ),
        sample_format => {
            println!("❌ Unsupported sample format for speaker loopback: {:?}", sample_format);
            return None;
        }
    };

    match stream {
        Ok(stream) => {
            println!("✅ Speaker loopback stream built successfully.");
            Some(stream)
        }
        Err(err) => {
            println!("❌ Error building speaker loopback stream: {}", err);
            None
        }
    }
}

pub fn to_mono_and_volume(samples: &[f32], channels: u16) -> (Vec<f32>, f32) {
    if samples.is_empty() {
        return (Vec::new(), 0.0);
    }

    let mono: Vec<f32> = if channels == 2 {
        samples
            .chunks(2)
            .map(|pair| {
                if pair.len() == 2 {
                    (pair[0] + pair[1]) * 0.5
                } else {
                    pair[0]
                }
            })
            .collect()
    } else {
        samples.to_vec()
    };

    let sum_squares: f32 = mono.iter().map(|s| s * s).sum();
    let rms = (sum_squares / mono.len() as f32).sqrt();

    (mono, rms)
}

pub fn resample_to_16k(mono_samples: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == 16000 || mono_samples.is_empty() {
        return mono_samples.to_vec();
    }

    let ratio = src_rate as f32 / 16000.0;
    let target_len = (mono_samples.len() as f32 / ratio) as usize;
    let mut resampled = Vec::with_capacity(target_len);

    for i in 0..target_len {
        let src_idx = i as f32 * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(mono_samples.len() - 1);
        let frac = src_idx - idx0 as f32;

        let sample = mono_samples[idx0] * (1.0 - frac) + mono_samples[idx1] * frac;
        resampled.push(sample);
    }

    resampled
}

pub fn create_wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    if samples.is_empty() {
        return Vec::new();
    }

    // 1. Calculate Peak Amplitude & RMS Level
    let mut raw_peak: f32 = 0.0;
    let mut sum_squares: f32 = 0.0;
    for &s in samples {
        let abs_s = s.abs();
        if abs_s > raw_peak {
            raw_peak = abs_s;
        }
        sum_squares += s * s;
    }
    let rms = (sum_squares / samples.len() as f32).sqrt();

    // 2. Calculate Automatic Gain Control (AGC) factor (target peak = 0.90, max boost = 20x)
    let gain = if raw_peak > 0.0001 {
        (0.90 / raw_peak).min(20.0)
    } else {
        1.0
    };

    println!(
        "📊 [AUDIO AGC] Raw Peak: {:.4} | RMS: {:.4} | Auto-Gain: {:.1}x",
        raw_peak, rms, gain
    );

    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * (num_channels as u32) * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = (samples.len() as u32) * (bits_per_sample as u32 / 8);
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity((44 + data_size) as usize);

    // RIFF Header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    // 3. Write PCM samples scaled by gain factor and clamped
    for &sample in samples {
        let boosted = sample * gain;
        let clamped = boosted.clamp(-1.0, 1.0);
        let int_sample = (clamped * 32767.0) as i16;
        wav.extend_from_slice(&int_sample.to_le_bytes());
    }

    wav
}

pub fn transcribe_groq(wav_bytes: Vec<u8>, api_key: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    let part = reqwest::blocking::multipart::Part::bytes(wav_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let form = reqwest::blocking::multipart::Form::new()
        .text("model", "whisper-large-v3-turbo")
        .text("prompt", "Transcribe the spoken audio cleanly.")
        .part("file", part);

    let response = client
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let err_text = response.text().unwrap_or_default();
        return Err(format!("Groq API Error: {}", err_text));
    }

    let json: serde_json::Value = response.json().map_err(|e| e.to_string())?;
    if let Some(text) = json.get("text").and_then(|t| t.as_str()) {
        Ok(text.trim().to_string())
    } else {
        Err("No text found in response".into())
    }
}

pub fn get_local_model_path() -> Result<std::path::PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;

    let candidates = [
        cwd.join("models").join("ggml-base.en.bin"),
        cwd.join("src-tauri").join("models").join("ggml-base.en.bin"),
        cwd.join("ggml-base.en.bin"),
    ];

    for path in &candidates {
        if path.exists() && std::fs::metadata(path).map(|m| m.len() > 100000).unwrap_or(false) {
            return Ok(path.clone());
        }
    }

    Err(format!(
        "❌ Local model 'ggml-base.en.bin' not found. Please place 'ggml-base.en.bin' in 'models/' folder at {:?}",
        cwd.join("models")
    ))
}

pub fn get_whisper_cli_path() -> Result<std::path::PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;

    let candidates = [
        cwd.join("bin").join("whisper-cli.exe"),
        cwd.join("bin").join("main.exe"),
        cwd.join("src-tauri").join("bin").join("whisper-cli.exe"),
        cwd.join("models").join("whisper-cli.exe"),
        cwd.join("whisper-cli.exe"),
    ];

    for path in &candidates {
        if path.exists() && std::fs::metadata(path).map(|m| m.len() > 100000).unwrap_or(false) {
            return Ok(path.clone());
        }
    }

    Err(format!(
        "❌ Local Whisper CLI 'whisper-cli.exe' not found. Please place 'whisper-cli.exe' in 'bin/' folder at {:?}",
        cwd.join("bin")
    ))
}

static LOCAL_WHISPER_LOCK: Mutex<()> = Mutex::new(());

pub fn transcribe_local(wav_bytes: Vec<u8>) -> Result<String, String> {
    // Acquire mutex lock (queues requests serially so no audio chunks are dropped & RAM remains ~150MB)
    let _lock = LOCAL_WHISPER_LOCK.lock().unwrap();

    let model_path = get_local_model_path()?;
    let cli_path = get_whisper_cli_path()?;

    let temp_dir = std::env::temp_dir().join("ai_assistant_stt");
    let _ = std::fs::create_dir_all(&temp_dir);

    // Generate unique timestamp filename to prevent race conditions
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_wav = temp_dir.join(format!("speech_input_{}.wav", nanos));
    std::fs::write(&temp_wav, &wav_bytes).map_err(|e| e.to_string())?;

    // Set current_dir to the folder containing whisper-cli.exe so it can find sibling DLLs
    let cli_dir = cli_path.parent().unwrap_or(std::path::Path::new("."));

    let output = std::process::Command::new(&cli_path)
        .current_dir(cli_dir)
        .arg("-m")
        .arg(&model_path)
        .arg("-f")
        .arg(&temp_wav)
        .arg("-l")
        .arg("en")
        .arg("-nt")
        .arg("-otxt")
        .output()
        .map_err(|e| format!("Failed to execute local Whisper CLI: {}", e))?;

    // Check generated txt files
    let txt_path1 = temp_wav.with_extension("wav.txt");
    let txt_path2 = temp_dir.join(format!("speech_input_{}.txt", nanos));

    let mut text = String::new();
    if txt_path1.exists() {
        text = std::fs::read_to_string(&txt_path1).unwrap_or_default();
        let _ = std::fs::remove_file(&txt_path1);
    } else if txt_path2.exists() {
        text = std::fs::read_to_string(&txt_path2).unwrap_or_default();
        let _ = std::fs::remove_file(&txt_path2);
    }

    let _ = std::fs::remove_file(&temp_wav);

    if !text.trim().is_empty() {
        return Ok(text.trim().to_string());
    }

    // Fallback: Parse stdout & stderr for recognized speech (filtering system backend logs)
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let clean_lines: Vec<String> = stdout
        .lines()
        .chain(stderr.lines())
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("whisper_")
                && !line.starts_with("system_info:")
                && !line.starts_with("main:")
                && !line.starts_with("load_backend:")
                && !line.starts_with("ggml_")
                && !line.starts_with("read_audio_data:")
                && !line.starts_with("alloc_tensor_range:")
                && !line.contains("GGML_ASSERT")
                && !line.contains("failed to allocate")
        })
        .map(|line| {
            if line.starts_with('[') && line.contains("-->") && line.contains(']') {
                if let Some(idx) = line.find(']') {
                    line[idx + 1..].trim().to_string()
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        })
        .filter(|line| !line.is_empty())
        .collect();

    let combined = clean_lines.join(" ");
    if !combined.trim().is_empty() {
        Ok(combined.trim().to_string())
    } else {
        Err("Local Whisper returned no text".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let is_recording = Arc::new(AtomicBool::new(true));
    let is_recording_bg = is_recording.clone();
    let groq_api_key = Arc::new(Mutex::new(String::new()));
    let groq_api_key_bg = groq_api_key.clone();
    let stt_engine = Arc::new(Mutex::new("groq".to_string()));
    let stt_engine_bg = stt_engine.clone();

    tauri::Builder::default()
        .manage(AppState {
            is_recording: is_recording.clone(),
            groq_api_key: groq_api_key.clone(),
            stt_engine: stt_engine.clone(),
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let (tx, rx) = channel::<AudioPacket>();

            // Spawn background thread for STT speech accumulator and event emitter
            thread::spawn(move || {
                println!("🚀 STT Pipeline Active!");
                let mut mic_buffer: Vec<f32> = Vec::new();
                let mut speaker_buffer: Vec<f32> = Vec::new();
                let mut mic_silence_counter = 0;
                let mut speaker_silence_counter = 0;

                // Rate-limiting cooldowns (Groq Free Tier has 20 RPM limit = 1 request per 3 seconds)
                let mut last_mic_stt_time = std::time::Instant::now() - std::time::Duration::from_secs(10);
                let mut last_speaker_stt_time = std::time::Instant::now() - std::time::Duration::from_secs(10);

                while let Ok(packet) = rx.recv() {
                    if is_recording_bg.load(Ordering::SeqCst) {
                        let (mono_samples, volume) = to_mono_and_volume(&packet.samples, packet.channels);
                        let source_str = match packet.source {
                            AudioSource::Microphone => "microphone",
                            AudioSource::Speaker => "speaker",
                        };

                        // Emit live volume level for UI meters
                        let _ = handle.emit(
                            "audio-level",
                            AudioLevelPayload {
                                source: source_str.to_string(),
                                volume,
                                samples_count: mono_samples.len(),
                            },
                        );

                        // Resample mono using ACTUAL packet sample rate to 16,000 Hz for Speech-to-Text
                        let samples_16k = resample_to_16k(&mono_samples, packet.sample_rate);

                        // Natural Utterance Endpointing VAD (Voice Activity & Natural Pause Detection)
                        match packet.source {
                            AudioSource::Microphone => {
                                let is_speech = volume > 0.0018; // Sensitive threshold for normal & quiet microphone speech

                                if is_speech {
                                    mic_buffer.extend_from_slice(&samples_16k);
                                    mic_silence_counter = 0;
                                } else if !mic_buffer.is_empty() {
                                    mic_buffer.extend_from_slice(&samples_16k);
                                    mic_silence_counter += 1;
                                }

                                let current_engine_check = stt_engine_bg.lock().unwrap().clone();
                                let (min_samples, max_samples, pause_endpoint_packets, cooldown_ms) = if current_engine_check == "local" {
                                    (12000, 240000, 25, 300)   // Local: ~0.75s min, ~500ms pause endpointing, 15s max force-flush
                                } else {
                                    (16000, 240000, 30, 2500)  // Groq: ~1.0s min, ~600ms pause endpointing, 15s max force-flush
                                };

                                let is_utterance_complete = (mic_silence_counter >= pause_endpoint_packets && mic_buffer.len() >= min_samples)
                                    || mic_buffer.len() >= max_samples;

                                if is_utterance_complete {
                                    if last_mic_stt_time.elapsed() >= std::time::Duration::from_millis(cooldown_ms) {
                                        last_mic_stt_time = std::time::Instant::now();
                                        
                                        let speech_chunk = mic_buffer.clone();
                                        let overlap_start = mic_buffer.len().saturating_sub(8000);
                                        mic_buffer = mic_buffer[overlap_start..].to_vec();
                                        mic_silence_counter = 0;

                                        let api_key = groq_api_key_bg.lock().unwrap().clone();
                                        let current_engine = stt_engine_bg.lock().unwrap().clone();
                                        let app_handle_clone = handle.clone();
                                        let capture_time = std::time::Instant::now();

                                        println!(
                                            "🎙️ [MIC VAD] Utterance captured! Samples: {} (~{:.1}s) | Engine: {}",
                                            speech_chunk.len(),
                                            speech_chunk.len() as f32 / 16000.0,
                                            current_engine
                                        );

                                        thread::spawn(move || {
                                            let queue_wait_ms = capture_time.elapsed().as_millis();
                                            
                                            // Check raw RMS before AGC to reject pure room noise/static
                                            let mut sum_sq = 0.0f32;
                                            for &s in &speech_chunk {
                                                sum_sq += s * s;
                                            }
                                            let raw_rms = (sum_sq / speech_chunk.len().max(1) as f32).sqrt();

                                            if raw_rms < 0.0012 {
                                                println!("⚠️ [MIC VAD] Raw RMS {:.5} below noise gate 0.0012, skipping silent chunk", raw_rms);
                                                return;
                                            }

                                            let wav_bytes = create_wav_bytes(&speech_chunk, 16000);

                                            let temp_dir = std::env::temp_dir().join("ai_assistant_debug");
                                            if let Ok(_) = std::fs::create_dir_all(&temp_dir) {
                                                let _ = std::fs::write(temp_dir.join("last_mic_speech.wav"), &wav_bytes);
                                            }

                                            let is_hallucination = |txt: &str| {
                                                let t = txt.trim().to_lowercase();
                                                t == "you" || t == "thanks." || t == "thank you." || t == "thanks for watching!" || t == "thanks for watching." || t == "subtitles by" || t == "bye." || t == "." || t.contains("amara.org")
                                            };

                                            if current_engine == "local" {
                                                let start_time = std::time::Instant::now();
                                                println!("💻 [LOCAL STT MIC] Transcribing mic audio chunk ({} bytes)...", wav_bytes.len());
                                                match transcribe_local(wav_bytes) {
                                                    Ok(text) => {
                                                        let total_latency = start_time.elapsed().as_millis();
                                                        println!("🎤 [LOCAL MIC RESULT] ({}) ({}ms): '{}'", if is_hallucination(&text) { "FILTERED" } else { "ACCEPTED" }, total_latency, text);
                                                        if text.len() > 2 && !is_hallucination(&text) {
                                                            let _ = app_handle_clone.emit(
                                                                "transcription",
                                                                TranscriptionPayload {
                                                                    source: "microphone".into(),
                                                                    text,
                                                                },
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        println!("❌ Local Mic STT Error: {}", e);
                                                    }
                                                }
                                            } else if !api_key.is_empty() {
                                                let api_start = std::time::Instant::now();
                                                println!("☁️ [GROQ STT MIC] Transcribing mic audio chunk via Groq API...");
                                                match transcribe_groq(wav_bytes, &api_key) {
                                                    Ok(text) => {
                                                        let api_latency_ms = api_start.elapsed().as_millis();
                                                        let total_latency_ms = capture_time.elapsed().as_millis();
                                                        println!("🎤 [GROQ MIC RESULT] Queue: {}ms | API: {}ms | Total: {}ms -> '{}'", queue_wait_ms, api_latency_ms, total_latency_ms, text);

                                                        if text.len() > 2 && !is_hallucination(&text) {
                                                            let _ = app_handle_clone.emit(
                                                                "transcription",
                                                                TranscriptionPayload {
                                                                    source: "microphone".into(),
                                                                    text,
                                                                },
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        let user_msg = if e.contains("rate_limit_exceeded") {
                                                            "⏳ Groq Free API Rate limit (20 RPM) reached. Switch to Local Mode or wait 3s...".to_string()
                                                        } else {
                                                            format!("⚠️ {}", e)
                                                        };
                                                        eprintln!("❌ Mic STT Error: {}", e);
                                                        let _ = app_handle_clone.emit(
                                                            "transcription",
                                                            TranscriptionPayload {
                                                                source: "microphone".into(),
                                                                text: user_msg,
                                                            },
                                                        );
                                                    }
                                                }
                                            } else {
                                                println!("⚠️ [MIC STT] Speech detected but Groq API key is empty and engine is 'groq'");
                                                let _ = app_handle_clone.emit(
                                                    "transcription",
                                                    TranscriptionPayload {
                                                        source: "microphone".into(),
                                                        text: "[Speech Detected] Please enter your Groq API Key or switch to Local Mode above!".into(),
                                                    },
                                                );
                                            }
                                        });
                                    }
                                } else if mic_silence_counter >= 60 {
                                    mic_buffer.clear();
                                    mic_silence_counter = 0;
                                }
                            }
                            AudioSource::Speaker => {
                                let is_speech = volume > 0.0006; // Highly sensitive threshold for YouTube / system audio

                                if is_speech {
                                    speaker_buffer.extend_from_slice(&samples_16k);
                                    speaker_silence_counter = 0;
                                } else if !speaker_buffer.is_empty() {
                                    speaker_buffer.extend_from_slice(&samples_16k);
                                    speaker_silence_counter += 1;
                                }

                                let current_engine_check = stt_engine_bg.lock().unwrap().clone();
                                let (min_samples, max_samples, pause_endpoint_packets, cooldown_ms) = if current_engine_check == "local" {
                                    (12000, 240000, 25, 300)
                                } else {
                                    (16000, 240000, 30, 2500)
                                };

                                let is_utterance_complete = (speaker_silence_counter >= pause_endpoint_packets && speaker_buffer.len() >= min_samples)
                                    || speaker_buffer.len() >= max_samples;

                                if is_utterance_complete {
                                    if last_speaker_stt_time.elapsed() >= std::time::Duration::from_millis(cooldown_ms) {
                                        last_speaker_stt_time = std::time::Instant::now();
                                        
                                        let speech_chunk = speaker_buffer.clone();
                                        let overlap_start = speaker_buffer.len().saturating_sub(8000);
                                        speaker_buffer = speaker_buffer[overlap_start..].to_vec();
                                        speaker_silence_counter = 0;

                                        let api_key = groq_api_key_bg.lock().unwrap().clone();
                                        let current_engine = stt_engine_bg.lock().unwrap().clone();
                                        let app_handle_clone = handle.clone();
                                        let capture_time = std::time::Instant::now();

                                        thread::spawn(move || {
                                             let queue_wait_ms = capture_time.elapsed().as_millis();

                                             // Check raw RMS before AGC to reject silence or empty overlap buffers
                                             let mut sum_sq = 0.0f32;
                                             for &s in &speech_chunk {
                                                 sum_sq += s * s;
                                             }
                                             let raw_rms = (sum_sq / speech_chunk.len().max(1) as f32).sqrt();

                                             if raw_rms < 0.0006 {
                                                 // Pure silence / zero amplitude; skip STT to prevent hallucination & empty logs
                                                 return;
                                             }

                                             let is_hallucination = |txt: &str| {
                                                 let t = txt.trim().to_lowercase();
                                                 t == "you" || t == "thanks." || t == "thank you." || t == "thanks for watching!" || t == "thanks for watching." || t == "subtitles by" || t == "bye." || t == "." || t.contains("amara.org")
                                             };

                                             let wav_bytes = create_wav_bytes(&speech_chunk, 16000);

                                             let temp_dir = std::env::temp_dir().join("ai_assistant_debug");
                                             if let Ok(_) = std::fs::create_dir_all(&temp_dir) {
                                                 let _ = std::fs::write(temp_dir.join("last_speaker_speech.wav"), &wav_bytes);
                                             }

                                             if current_engine == "local" {
                                                 let start_time = std::time::Instant::now();
                                                 println!("💻 [LOCAL STT SPEAKER] Transcribing speaker utterance ({} samples, {:.2}s)...", speech_chunk.len(), speech_chunk.len() as f32 / 16000.0);
                                                 match transcribe_local(wav_bytes) {
                                                     Ok(text) => {
                                                         let total_latency = start_time.elapsed().as_millis();
                                                         if text.len() > 2 && !is_hallucination(&text) {
                                                             println!("🔊 [LOCAL SPEAKER] ({}ms): {}", total_latency, text);
                                                             let _ = app_handle_clone.emit(
                                                                 "transcription",
                                                                 TranscriptionPayload {
                                                                     source: "speaker".into(),
                                                                     text,
                                                                 },
                                                             );
                                                         }
                                                     }
                                                     Err(e) => {
                                                         eprintln!("❌ Local Speaker STT Error: {}", e);
                                                     }
                                                 }
                                             } else if !api_key.is_empty() {
                                                let api_start = std::time::Instant::now();
                                                match transcribe_groq(wav_bytes, &api_key) {
                                                    Ok(text) => {
                                                        let api_latency_ms = api_start.elapsed().as_millis();
                                                        let total_latency_ms = capture_time.elapsed().as_millis();

                                                        if text.len() > 2 && !is_hallucination(&text) {
                                                            println!("🔊 [GROQ SPEAKER] Queue: {}ms | API: {}ms | Total: {}ms -> {}", queue_wait_ms, api_latency_ms, total_latency_ms, text);
                                                            let _ = app_handle_clone.emit(
                                                                "transcription",
                                                                TranscriptionPayload {
                                                                    source: "speaker".into(),
                                                                    text,
                                                                },
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        let user_msg = if e.contains("rate_limit_exceeded") {
                                                            "⏳ Groq Free API Rate limit (20 RPM) reached. Switch to Local Mode or wait 3s...".to_string()
                                                        } else {
                                                            format!("⚠️ {}", e)
                                                        };
                                                        eprintln!("❌ Speaker STT Error: {}", e);
                                                        let _ = app_handle_clone.emit(
                                                            "transcription",
                                                            TranscriptionPayload {
                                                                source: "speaker".into(),
                                                                text: user_msg,
                                                            },
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                    }
                                } else if speaker_silence_counter >= 60 {
                                    speaker_buffer.clear();
                                    speaker_silence_counter = 0;
                                }
                            }
                        }
                    }
                }
            });

            // Start Mic Stream
            let mic_tx = tx.clone();
            if let Some(mic) = get_default_microphone() {
                print_microphone_config(&mic);
                if let Some(mic_stream) = build_microphone_stream(&mic, mic_tx) {
                    let _ = mic_stream.play();
                    Box::leak(Box::new(mic_stream));
                }
            }

            // Start Speaker Stream
            if let Some(speaker) = get_default_speaker() {
                if let Some(speaker_stream) = build_speaker_loopback_stream(&speaker, tx) {
                    let _ = speaker_stream.play();
                    Box::leak(Box::new(speaker_stream));
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            start_audio_capture,
            stop_audio_capture,
            get_audio_status,
            set_groq_api_key,
            set_stt_engine
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
