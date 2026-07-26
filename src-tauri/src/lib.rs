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

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let is_recording = Arc::new(AtomicBool::new(true));
    let is_recording_bg = is_recording.clone();
    let groq_api_key = Arc::new(Mutex::new(String::new()));
    let groq_api_key_bg = groq_api_key.clone();

    tauri::Builder::default()
        .manage(AppState {
            is_recording: is_recording.clone(),
            groq_api_key: groq_api_key.clone(),
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

                        // Voice Activity Detection (VAD) & Speech Accumulation
                        match packet.source {
                            AudioSource::Microphone => {
                                if volume > 0.010 {
                                    mic_buffer.extend_from_slice(&samples_16k);
                                    mic_silence_counter = 0;
                                } else if !mic_buffer.is_empty() {
                                    mic_silence_counter += 1;
                                }

                                // Accumulate ~2.0-3.0s of speech (32,000 to 48,000 samples) or silence after 1.5s
                                let is_chunk_ready = mic_buffer.len() >= 48000
                                    || (mic_silence_counter >= 15 && mic_buffer.len() >= 24000);

                                if is_chunk_ready {
                                    // Check 3.0-second rate-limit cooldown before firing Groq request
                                    if last_mic_stt_time.elapsed() >= std::time::Duration::from_millis(3000) {
                                        last_mic_stt_time = std::time::Instant::now();
                                        let speech_chunk = std::mem::take(&mut mic_buffer);
                                        mic_silence_counter = 0;

                                        let api_key = groq_api_key_bg.lock().unwrap().clone();
                                        let app_handle_clone = handle.clone();
                                        let capture_time = std::time::Instant::now();

                                        thread::spawn(move || {
                                            let queue_wait_ms = capture_time.elapsed().as_millis();
                                            let wav_bytes = create_wav_bytes(&speech_chunk, 16000);

                                            // STAGE TEST: Save debug WAV to disk to inspect audio quality before API send
                                            if let Ok(_) = std::fs::create_dir_all("debug_audio") {
                                                let _ = std::fs::write("debug_audio/last_mic_speech.wav", &wav_bytes);
                                            }

                                            if !api_key.is_empty() {
                                                let api_start = std::time::Instant::now();
                                                match transcribe_groq(wav_bytes, &api_key) {
                                                    Ok(text) => {
                                                        let api_latency_ms = api_start.elapsed().as_millis();
                                                        let total_latency_ms = capture_time.elapsed().as_millis();

                                                        println!(
                                                            "⏱️ [LATENCY - MIC] Queue Wait: {}ms | Groq API: {}ms | Total: {}ms",
                                                            queue_wait_ms, api_latency_ms, total_latency_ms
                                                        );

                                                        if text.len() > 2 && text != "you" && text != "Thanks." && text != "Thank you." {
                                                            println!("🎤 Transcribed Mic: {}", text);
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
                                                            "⏳ Groq Free API Rate limit (20 RPM) reached. Waiting 3s...".to_string()
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
                                                let _ = app_handle_clone.emit(
                                                    "transcription",
                                                    TranscriptionPayload {
                                                        source: "microphone".into(),
                                                        text: "[Speech Detected] Please enter your Groq API Key above to see transcriptions!".into(),
                                                    },
                                                );
                                            }
                                        });
                                    }
                                } else if mic_silence_counter >= 20 {
                                    mic_buffer.clear();
                                    mic_silence_counter = 0;
                                }
                            }
                            AudioSource::Speaker => {
                                if volume > 0.012 {
                                    speaker_buffer.extend_from_slice(&samples_16k);
                                    speaker_silence_counter = 0;
                                } else if !speaker_buffer.is_empty() {
                                    speaker_silence_counter += 1;
                                }

                                let is_chunk_ready = speaker_buffer.len() >= 48000
                                    || (speaker_silence_counter >= 15 && speaker_buffer.len() >= 24000);

                                if is_chunk_ready {
                                    if last_speaker_stt_time.elapsed() >= std::time::Duration::from_millis(3000) {
                                        last_speaker_stt_time = std::time::Instant::now();
                                        let speech_chunk = std::mem::take(&mut speaker_buffer);
                                        speaker_silence_counter = 0;

                                        let api_key = groq_api_key_bg.lock().unwrap().clone();
                                        let app_handle_clone = handle.clone();
                                        let capture_time = std::time::Instant::now();

                                        thread::spawn(move || {
                                            let queue_wait_ms = capture_time.elapsed().as_millis();
                                            let wav_bytes = create_wav_bytes(&speech_chunk, 16000);

                                            // STAGE TEST: Save debug WAV to disk for speaker loopback audio inspection
                                            if let Ok(_) = std::fs::create_dir_all("debug_audio") {
                                                let _ = std::fs::write("debug_audio/last_speaker_speech.wav", &wav_bytes);
                                            }

                                            if !api_key.is_empty() {
                                                let api_start = std::time::Instant::now();
                                                match transcribe_groq(wav_bytes, &api_key) {
                                                    Ok(text) => {
                                                        let api_latency_ms = api_start.elapsed().as_millis();
                                                        let total_latency_ms = capture_time.elapsed().as_millis();

                                                        println!(
                                                            "⏱️ [LATENCY - SPEAKER] Queue Wait: {}ms | Groq API: {}ms | Total: {}ms",
                                                            queue_wait_ms, api_latency_ms, total_latency_ms
                                                        );

                                                        if text.len() > 2 && text != "you" && text != "Thanks." {
                                                            println!("🔊 Transcribed Speaker: {}", text);
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
                                                            "⏳ Groq Free API Rate limit (20 RPM) reached. Waiting 3s...".to_string()
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
                                } else if speaker_silence_counter >= 20 {
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
            set_groq_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

