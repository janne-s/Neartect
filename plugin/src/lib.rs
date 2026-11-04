use base64::Engine;
use crossbeam_channel::{bounded, Receiver, Sender};
use include_dir::{include_dir, Dir};
use nih_plug::prelude::*;
use nih_plug_webview::{http, HTMLSource, Key, WebViewEditor};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::borrow::Cow;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

pub mod convolver;
use convolver::{Convolver, Filter, BLOCK};

static SOURCES: Dir = include_dir!("$CARGO_MANIFEST_DIR/../src");
static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/../assets");
static PAGE: &[u8] = include_bytes!("../../index.html");
static THEME: &[u8] = include_bytes!("../../theme.css");

const WINDOW_SIZE: (u32, u32) = (1200, 760);
const SMALLEST_WINDOW: (u32, u32) = (640, 420);
const FILTER_QUEUE: usize = 4;
const LOG_FILE: &str = "neartect-plugin.log";
const BYPASS_SECONDS: f32 = 0.01;

#[derive(Default, Serialize, Deserialize, Clone)]
struct StoredFilter {
    sample_rate: f32,
    left: Vec<f32>,
    right: Vec<f32>,
}

#[derive(Params)]
struct NeartectParams {
    #[persist = "space"]
    space: Arc<RwLock<String>>,
    #[persist = "filter"]
    filter: Arc<RwLock<StoredFilter>>,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "output"]
    output: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    bypass_changed: Arc<AtomicBool>,
}

impl Default for NeartectParams {
    fn default() -> Self {
        let bypass_changed = Arc::new(AtomicBool::new(false));
        let flag = bypass_changed.clone();
        Self {
            space: Arc::new(RwLock::new(String::new())),
            filter: Arc::new(RwLock::new(StoredFilter::default())),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            output: FloatParam::new(
                "Output",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-36.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-36.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(30.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            bypass: BoolParam::new("Bypass", false)
                .make_bypass()
                .with_callback(Arc::new(move |_| flag.store(true, Ordering::Relaxed))),
            bypass_changed,
        }
    }
}

struct Neartect {
    params: Arc<NeartectParams>,
    convolver: Convolver,
    dry: [Vec<f32>; 2],
    dry_at: usize,
    inputs: usize,
    heard: f32,
    sample_rate: Arc<AtomicU32>,
    filters_in: Receiver<Box<Filter>>,
    filters_out: Sender<Box<Filter>>,
    retired_out: Sender<Box<Filter>>,
    retired_in: Receiver<Box<Filter>>,
}

impl Default for Neartect {
    fn default() -> Self {
        let (filters_out, filters_in) = bounded(FILTER_QUEUE);
        let (retired_out, retired_in) = bounded(FILTER_QUEUE);
        Self {
            params: Arc::new(NeartectParams::default()),
            convolver: Convolver::new(48000.0),
            dry: [vec![0.0; BLOCK], vec![0.0; BLOCK]],
            dry_at: 0,
            inputs: 2,
            heard: 1.0,
            sample_rate: Arc::new(AtomicU32::new(48000.0f32.to_bits())),
            filters_in,
            filters_out,
            retired_out,
            retired_in,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Message {
    Ready,
    Space {
        json: String,
    },
    #[serde(rename_all = "camelCase")]
    Filter {
        sample_rate_hz: f32,
        left: Vec<f32>,
        right: Vec<f32>,
    },
    Resize {
        width: u32,
        height: u32,
    },
    Log {
        text: String,
    },
    Bypass {
        on: bool,
    },
    Open,
    Save {
        name: String,
        base64: String,
    },
}

fn log(text: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join(LOG_FILE))
    {
        let _ = writeln!(file, "{text}");
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html",
        Some("js") | Some("mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") => "image/jpeg",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn served(path: &str) -> Option<&'static [u8]> {
    let path = path.trim_start_matches('/');
    match path {
        "" | "index.html" => Some(PAGE),
        "theme.css" => Some(THEME),
        _ => {
            if let Some(rest) = path.strip_prefix("src/") {
                SOURCES.get_file(rest).map(|f| f.contents())
            } else if let Some(rest) = path.strip_prefix("assets/") {
                ASSETS.get_file(rest).map(|f| f.contents())
            } else {
                None
            }
        }
    }
}

impl Plugin for Neartect {
    const NAME: &'static str = "Neartect";
    const VENDOR: &'static str = "Janne Särkelä";
    const URL: &'static str = "https://janne-s.github.io/Neartect/";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let rate = buffer_config.sample_rate;
        self.inputs = layout.main_input_channels.map_or(2, |n| n.get() as usize);
        self.sample_rate.store(rate.to_bits(), Ordering::Relaxed);
        self.convolver = Convolver::new(rate);
        self.dry = [vec![0.0; BLOCK], vec![0.0; BLOCK]];
        let stored = self
            .params
            .filter
            .read()
            .map(|f| f.clone())
            .unwrap_or_default();
        if stored.sample_rate == rate && !stored.left.is_empty() {
            self.convolver
                .set_now(Box::new(Filter::new(rate, &stored.left, &stored.right)));
        }
        context.set_latency_samples(BLOCK as u32);
        true
    }

    fn reset(&mut self) {
        let rate = f32::from_bits(self.sample_rate.load(Ordering::Relaxed));
        let mut fresh = Convolver::new(rate);
        let stored = self
            .params
            .filter
            .read()
            .map(|f| f.clone())
            .unwrap_or_default();
        if stored.sample_rate == rate && !stored.left.is_empty() {
            fresh.set_now(Box::new(Filter::new(rate, &stored.left, &stored.right)));
        }
        self.convolver = fresh;
        self.dry.iter_mut().for_each(|d| d.fill(0.0));
        self.dry_at = 0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        while let Ok(filter) = self.filters_in.try_recv() {
            self.convolver.offer(filter);
        }
        while let Some(old) = self.convolver.take_retired() {
            if let Err(error) = self.retired_out.try_send(old) {
                drop(error.into_inner());
            }
        }
        let stereo_input = self.inputs > 1;
        let rate = f32::from_bits(self.sample_rate.load(Ordering::Relaxed));
        let step = 1.0 / (BYPASS_SECONDS * rate);
        let target = if self.params.bypass.value() { 0.0 } else { 1.0 };
        for mut frame in buffer.iter_samples() {
            self.heard += (target - self.heard).clamp(-step, step);
            let mix = self.params.mix.smoothed.next() * self.heard;
            let gain = self.params.output.smoothed.next();
            let left = *frame.get_mut(0).expect("a main channel");
            let right = if stereo_input {
                *frame.get_mut(1).expect("a second channel")
            } else {
                left
            };
            let wet = self.convolver.process(0.5 * (left + right));
            let slot = self.dry_at;
            self.dry_at = (self.dry_at + 1) % BLOCK;
            let dry = [self.dry[0][slot], self.dry[1][slot]];
            self.dry[0][slot] = left;
            self.dry[1][slot] = right;
            for (ear, sample) in frame.iter_mut().enumerate().take(2) {
                *sample = gain * (dry[ear] * (1.0 - mix) + wet[ear] * mix);
            }
        }
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let sample_rate = self.sample_rate.clone();
        let filters_out = self.filters_out.clone();
        let retired_in = self.retired_in.clone();
        let editor = WebViewEditor::new(HTMLSource::URL("neartect://localhost/index.html"), WINDOW_SIZE)
            .with_background_color((32, 32, 32, 255))
            .with_developer_mode(true)
            .with_custom_protocol("neartect".to_owned(), |request| {
                let path = request.uri().path();
                Ok(match served(path) {
                    Some(bytes) => http::Response::builder().header("Content-Type", content_type(path)).body(Cow::Borrowed(bytes)).expect("a valid response"),
                    None => http::Response::builder().status(404).body(Cow::Borrowed(&[][..])).expect("a valid response"),
                })
            })
            .with_keyboard_handler(|event| event.key == Key::Escape)
            .with_event_loop(move |window, setter, base| {
                while retired_in.try_recv().is_ok() {}
                if params.bypass_changed.swap(false, Ordering::Relaxed) {
                    window.send_json(json!({ "type": "bypass", "on": params.bypass.value() }));
                }
                while let Ok(value) = window.next_event() {
                    match serde_json::from_value::<Message>(value) {
                        Ok(Message::Ready) => {
                            let space = params.space.read().map(|s| s.clone()).unwrap_or_default();
                            let rate = f32::from_bits(sample_rate.load(Ordering::Relaxed));
                            window.send_json(json!({ "type": "init", "sampleRateHz": rate, "bypass": params.bypass.value(), "space": if space.is_empty() { None } else { Some(space) } }));
                        }
                        Ok(Message::Space { json }) => {
                            if let Ok(mut space) = params.space.write() {
                                *space = json;
                            }
                        }
                        Ok(Message::Filter { sample_rate_hz, left, right }) => {
                            let rate = f32::from_bits(sample_rate.load(Ordering::Relaxed));
                            if sample_rate_hz != rate {
                                continue;
                            }
                            let _ = filters_out.try_send(Box::new(Filter::new(rate, &left, &right)));
                            if let Ok(mut stored) = params.filter.write() {
                                *stored = StoredFilter { sample_rate: rate, left, right };
                            }
                        }
                        Ok(Message::Resize { width, height }) => window.resize(base, width.max(SMALLEST_WINDOW.0), height.max(SMALLEST_WINDOW.1)),
                        Ok(Message::Log { text }) => log(&text),
                        Ok(Message::Bypass { on }) => {
                            setter.begin_set_parameter(&params.bypass);
                            setter.set_parameter(&params.bypass, on);
                            setter.end_set_parameter(&params.bypass);
                            params.bypass_changed.store(false, Ordering::Relaxed);
                        }
                        Ok(Message::Open) => {
                            if let Some(path) = rfd::FileDialog::new().add_filter("Neartect space", &["json"]).pick_file() {
                                match std::fs::read_to_string(&path) {
                                    Ok(text) => window.send_json(json!({ "type": "opened", "text": text })),
                                    Err(error) => log(&format!("open {}: {error}", path.display())),
                                }
                            }
                        }
                        Ok(Message::Save { name, base64 }) => match base64::engine::general_purpose::STANDARD.decode(base64) {
                            Ok(bytes) => {
                                if let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                                    if let Err(error) = std::fs::write(&path, bytes) {
                                        log(&format!("save {}: {error}", path.display()));
                                    }
                                }
                            }
                            Err(error) => log(&format!("save {name}: {error}")),
                        },
                        Err(error) => log(&format!("unreadable message: {error}")),
                    }
                }
            });
        Some(Box::new(editor))
    }
}

impl ClapPlugin for Neartect {
    const CLAP_ID: &'static str = "io.github.janne-s.neartect";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("The near space around a microphone");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Reverb,
    ];
}

impl Vst3Plugin for Neartect {
    const VST3_CLASS_ID: [u8; 16] = *b"NeartectNearSpce";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_clap!(Neartect);
nih_export_vst3!(Neartect);
