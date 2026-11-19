use base64::Engine;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use include_dir::{include_dir, Dir};
use nih_plug::prelude::*;
use nih_plug_webview::{http, HTMLSource, Key, WebViewEditor};
use serde::{Deserialize, Serialize};
use serde_json::json;
use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle};
use raw_window_handle_05::HasRawWindowHandle;
use rfd::{AsyncFileDialog, FileHandle};
use std::borrow::Cow;
use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::ptr::NonNull;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::task::{Context, Poll, Waker};

pub mod compute;
pub mod convolver;
pub mod params;
pub mod scenes;
use params::{Latch, SlotParams};
use compute::{Builder, Computed, Outcome};
use convolver::{Convolver, Filter, BLOCK};

static PAGE_SOURCES: Dir = include_dir!("$CARGO_MANIFEST_DIR/../src");
static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/../assets");
static PAGE: &[u8] = include_bytes!("../../index.html");
static THEME: &[u8] = include_bytes!("../../theme.css");

const WINDOW_SIZE: (u32, u32) = (1200, 760);
const SMALLEST_WINDOW: (u32, u32) = (640, 420);
const RETIRED_QUEUE: usize = 8;
const LOG_FILE: &str = "neartect-plugin.log";
const BYPASS_SECONDS: f32 = 0.01;
const SIGNAL_GAIN: f32 = 0.35;

pub struct Signal {
    samples: Vec<f32>,
    sample_rate: f32,
}

#[derive(Default)]
struct Audition {
    signal: String,
    file: Option<String>,
    samples: Option<Arc<Signal>>,
    playing: bool,
}

type SignalChange = Mutex<Option<Option<Arc<Signal>>>>;

enum Retired {
    Filter(Box<Filter>),
    Signal(Arc<Signal>),
    Bank(Box<Bank>),
}

const SOURCES: usize = neartect_core::space::MOST_SOURCES;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Input {
    Main,
    MainLeft,
    MainRight,
    Side,
    SideLeft,
    SideRight,
}

impl Input {
    fn of(name: &str) -> Input {
        match name {
            "mainLeft" => Input::MainLeft,
            "mainRight" => Input::MainRight,
            "side" => Input::Side,
            "sideLeft" => Input::SideLeft,
            "sideRight" => Input::SideRight,
            _ => Input::Main,
        }
    }

    #[inline]
    fn pick(self, main: [f32; 2], side: [f32; 2]) -> f32 {
        match self {
            Input::Main => 0.5 * (main[0] + main[1]),
            Input::MainLeft => main[0],
            Input::MainRight => main[1],
            Input::Side => 0.5 * (side[0] + side[1]),
            Input::SideLeft => side[0],
            Input::SideRight => side[1],
        }
    }
}

pub struct Bank {
    filters: [Option<Box<Filter>>; SOURCES],
    inputs: [Input; SOURCES],
}

impl Bank {
    fn of<'a>(rate: f32, sources: impl Iterator<Item = (usize, &'a str, &'a [f32], &'a [f32])>) -> Box<Bank> {
        let mut bank = Box::new(Bank { filters: Default::default(), inputs: [Input::Main; SOURCES] });
        for (number, input, left, right) in sources {
            if (1..=SOURCES).contains(&number) {
                bank.filters[number - 1] = Some(Box::new(Filter::new(rate, left, right)));
                bank.inputs[number - 1] = Input::of(input);
            }
        }
        for filter in bank.filters.iter_mut().filter(|f| f.is_none()) {
            *filter = Some(Box::new(Filter::new(rate, &[0.0], &[0.0])));
        }
        bank
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
struct StoredSource {
    number: usize,
    input: String,
    left: Vec<f32>,
    right: Vec<f32>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
struct StoredFilter {
    sample_rate: f32,
    #[serde(default)]
    left: Vec<f32>,
    #[serde(default)]
    right: Vec<f32>,
    #[serde(default)]
    sources: Vec<StoredSource>,
}

impl StoredFilter {
    fn bank(&self, rate: f32) -> Option<Box<Bank>> {
        if self.sample_rate != rate {
            return None;
        }
        if !self.sources.is_empty() {
            return Some(Bank::of(rate, self.sources.iter().map(|s| (s.number, s.input.as_str(), s.left.as_slice(), s.right.as_slice()))));
        }
        (!self.left.is_empty()).then(|| Bank::of(rate, std::iter::once((1, "main", self.left.as_slice(), self.right.as_slice()))))
    }
}

#[derive(Params)]
pub struct NeartectParams {
    #[persist = "space"]
    space: Arc<RwLock<String>>,
    #[persist = "filter"]
    filter: Arc<RwLock<StoredFilter>>,
    #[persist = "synced"]
    synced: Arc<RwLock<bool>>,
    #[persist = "camera"]
    camera: Arc<RwLock<serde_json::Value>>,
    #[persist = "latch"]
    latch: Arc<RwLock<Latch>>,
    #[id = "scene"]
    scene: IntParam,
    scene_changed: Arc<AtomicBool>,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "output"]
    output: FloatParam,
    #[id = "bypass"]
    bypass: BoolParam,
    bypass_changed: Arc<AtomicBool>,
    #[nested]
    pub slots: SlotParams,
}

impl NeartectParams {
    fn new(wake: Sender<()>) -> Self {
        let bypass_changed = Arc::new(AtomicBool::new(false));
        let flag = bypass_changed.clone();
        let scene_changed = Arc::new(AtomicBool::new(false));
        let (scene_flag, scene_wake) = (scene_changed.clone(), wake.clone());
        let slots = SlotParams::new(wake);
        let moves = slots.moves.clone();
        Self {
            space: Arc::new(RwLock::new(String::new())),
            filter: Arc::new(RwLock::new(StoredFilter::default())),
            synced: Arc::new(RwLock::new(false)),
            camera: Arc::new(RwLock::new(serde_json::Value::Null)),
            latch: Arc::new(RwLock::new(Latch::default())),
            scene: IntParam::new("Scene", 1, IntRange::Linear { min: 1, max: scenes::SCENES as i32 }).with_callback(Arc::new(move |_| {
                moves.mark_switch();
                scene_flag.store(true, Ordering::Release);
                let _ = scene_wake.try_send(());
            })),
            scene_changed,
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
            slots,
        }
    }

    fn live(&self) -> Option<(usize, String)> {
        let document = self.space.read().map(|s| s.clone()).unwrap_or_default();
        if document.is_empty() {
            return None;
        }
        let (number, scene) = scenes::chosen(&document, self.scene.value() as usize).unwrap_or((1, document));
        if !self.synced.read().is_ok_and(|s| *s) {
            return Some((number, scene));
        }
        let values = self.slots.values();
        let latch = match self.latch.write() {
            Ok(mut latch) => {
                latch.enter(number, &values, |i| self.slots.moves.moving_at_switch(i));
                latch.clone()
            }
            Err(_) => Latch::default(),
        };
        let live = params::applied_where(&scene, &self.slots.specs, &values, |i| latch.overrides(i, values[i])).unwrap_or(scene);
        Some((number, live))
    }

    fn live_space(&self) -> String {
        self.live().map(|(_, scene)| scene).unwrap_or_default()
    }

    fn live_document(&self) -> String {
        let document = self.space.read().map(|s| s.clone()).unwrap_or_default();
        match self.live() {
            Some((number, scene)) => scenes::with_active(&document, number, &scene).unwrap_or(scene),
            None => document,
        }
    }
}

type Delivery = (u64, Option<Box<Bank>>);

#[derive(Default)]
struct Mailbox {
    slot: Mutex<Option<Delivery>>,
    ready: Condvar,
}

impl Mailbox {
    fn put(&self, generation: u64, filter: Option<Box<Bank>>) {
        if let Ok(mut slot) = self.slot.lock() {
            if slot.as_ref().is_none_or(|(held, _)| *held <= generation) {
                *slot = Some((generation, filter));
            }
        }
        self.ready.notify_all();
    }
}

struct Neartect {
    params: Arc<NeartectParams>,
    convolvers: Vec<Convolver>,
    inputs: [Input; SOURCES],
    dry: [Vec<f32>; 2],
    dry_at: usize,
    main_channels: usize,
    heard: f32,
    sample_rate: Arc<AtomicU32>,
    mailbox: Arc<Mailbox>,
    applied: u64,
    offline: bool,
    builder: Arc<Builder>,
    retired_out: Sender<Retired>,
    clock: Arc<AtomicU64>,
    audition: Arc<Mutex<Audition>>,
    signal_change: Arc<SignalChange>,
    signal: Option<Arc<Signal>>,
    phase: f64,
    page_out: Sender<Outcome>,
    page_in: Receiver<Outcome>,
    editor_open: Arc<AtomicBool>,
    wake: Sender<()>,
}

struct OpenWindow(Arc<AtomicBool>, Sender<()>);

impl Drop for OpenWindow {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
        let _ = self.1.try_send(());
    }
}

struct WatchedEditor {
    inner: WebViewEditor,
    open: Arc<AtomicBool>,
    wake: Sender<()>,
}

impl Editor for WatchedEditor {
    fn spawn(&self, parent: ParentWindowHandle, context: Arc<dyn GuiContext>) -> Box<dyn std::any::Any + Send> {
        let window = self.inner.spawn(parent, context);
        Box::new((window, OpenWindow(self.open.clone(), self.wake.clone())))
    }

    fn size(&self) -> (u32, u32) {
        self.inner.size()
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        self.inner.set_scale_factor(factor)
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        self.inner.param_value_changed(id, normalized_value)
    }

    fn param_modulation_changed(&self, id: &str, modulation_offset: f32) {
        self.inner.param_modulation_changed(id, modulation_offset)
    }

    fn param_values_changed(&self) {
        self.inner.param_values_changed()
    }
}

impl Default for Neartect {
    fn default() -> Self {
        let (retired_out, retired_in) = bounded(RETIRED_QUEUE);
        let (page_out, page_in) = unbounded();
        let (wake_out, wake_in) = bounded(1);
        let wake = wake_out.clone();
        let params = Arc::new(NeartectParams::new(wake_out));
        let sample_rate = Arc::new(AtomicU32::new(48000.0f32.to_bits()));
        let editor_open = Arc::new(AtomicBool::new(false));
        let source = {
            let (params, sample_rate, editor_open) = (params.clone(), sample_rate.clone(), editor_open.clone());
            move || {
                if editor_open.load(Ordering::Acquire) {
                    return None;
                }
                let space = params.live_space();
                (!space.is_empty()).then(|| (space, Some(f32::from_bits(sample_rate.load(Ordering::Relaxed)) as f64)))
            }
        };
        let mailbox = Arc::new(Mailbox::default());
        let clock = Arc::new(AtomicU64::new(0));
        let seconds = {
            let (clock, sample_rate) = (clock.clone(), sample_rate.clone());
            move || clock.load(Ordering::Relaxed) as f64 / f32::from_bits(sample_rate.load(Ordering::Relaxed)) as f64
        };
        let deliver = {
            let (params, sample_rate, mailbox, page_out) = (params.clone(), sample_rate.clone(), mailbox.clone(), page_out.clone());
            move |outcome: Outcome| {
                let (generation, page) = match &outcome {
                    Outcome::Built { generation, page, .. } | Outcome::Cancelled { generation, page } | Outcome::Failed { generation, page, .. } => (*generation, *page),
                };
                let rate = f32::from_bits(sample_rate.load(Ordering::Relaxed));
                let filter = match &outcome {
                    Outcome::Built { computed, .. } if computed.sample_rate_hz as f32 == rate => {
                        let sources: Vec<StoredSource> = computed
                            .sources
                            .iter()
                            .map(|s| StoredSource { number: s.number, input: s.input.to_string(), left: s.stereo[0].clone(), right: s.stereo[1].clone() })
                            .collect();
                        let bank = Bank::of(rate, sources.iter().map(|s| (s.number, s.input.as_str(), s.left.as_slice(), s.right.as_slice())));
                        if let Ok(mut stored) = params.filter.write() {
                            *stored = StoredFilter { sample_rate: rate, sources, ..StoredFilter::default() };
                        }
                        Some(bank)
                    }
                    _ => None,
                };
                mailbox.put(generation, filter);
                if page.is_some() {
                    let _ = page_out.send(outcome);
                }
            }
        };
        Self {
            builder: Arc::new(Builder::new(retired_in, wake_in, source, seconds, deliver)),
            params,
            convolvers: (0..SOURCES).map(|_| Convolver::new(48000.0)).collect(),
            inputs: [Input::Main; SOURCES],
            dry: [vec![0.0; BLOCK], vec![0.0; BLOCK]],
            dry_at: 0,
            main_channels: 2,
            heard: 1.0,
            sample_rate,
            mailbox,
            applied: 0,
            offline: false,
            retired_out,
            clock,
            audition: Arc::new(Mutex::new(Audition { signal: "pink".into(), ..Audition::default() })),
            signal_change: Arc::new(Mutex::new(None)),
            signal: None,
            phase: 0.0,
            page_out,
            page_in,
            editor_open,
            wake,
        }
    }
}

impl Neartect {
    fn take_delivery(&mut self, waiting: bool) {
        let latest = self.builder.latest();
        let taken = if waiting && self.applied < latest {
            let Ok(slot) = self.mailbox.slot.lock() else { return };
            let Ok(mut slot) = self.mailbox.ready.wait_while(slot, |slot| slot.as_ref().is_none_or(|(g, _)| *g < latest)) else { return };
            slot.take()
        } else {
            self.mailbox.slot.try_lock().ok().and_then(|mut slot| slot.take())
        };
        if let Some((generation, bank)) = taken {
            self.applied = self.applied.max(generation);
            if let Some(bank) = bank {
                self.offer(bank, false);
            }
        }
    }

    fn offer(&mut self, mut bank: Box<Bank>, now: bool) {
        self.inputs = bank.inputs;
        for (convolver, filter) in self.convolvers.iter_mut().zip(bank.filters.iter_mut()) {
            if let Some(filter) = filter.take() {
                if now {
                    convolver.set_now(filter);
                } else {
                    convolver.offer(filter);
                }
            }
        }
        if let Err(error) = self.retired_out.try_send(Retired::Bank(bank)) {
            drop(error.into_inner());
        }
    }
}

fn computed_json(id: u64, computed: &Computed, milliseconds: f64) -> serde_json::Value {
    let sources: Vec<serde_json::Value> = computed.sources.iter().map(|s| json!({ "number": s.number, "name": s.name, "input": s.input, "first": s.first, "count": s.count })).collect();
    json!({ "type": "computed", "id": id, "sampleRateHz": computed.sample_rate_hz, "channels": computed.channels, "names": computed.names, "pans": computed.pans, "sources": sources, "milliseconds": milliseconds })
}

fn outcome_json(outcome: &Outcome) -> Option<serde_json::Value> {
    Some(match outcome {
        Outcome::Built { page, computed, milliseconds, .. } => computed_json((*page)?, computed, *milliseconds),
        Outcome::Cancelled { page, .. } => json!({ "type": "computed", "id": (*page)?, "cancelled": true }),
        Outcome::Failed { page, message, .. } => json!({ "type": "computed", "id": (*page)?, "error": message }),
    })
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Message {
    Ready,
    Space {
        json: String,
    },
    Compute {
        id: u64,
        json: String,
        purpose: String,
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
    Set {
        id: String,
        value: f64,
    },
    End,
    Synced,
    Scene {
        number: i32,
    },
    Camera {
        camera: serde_json::Value,
    },
    Audition {
        signal: String,
        file: Option<String>,
        playing: bool,
        samples: Option<String>,
        rate: Option<f32>,
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

type Chosen = Pin<Box<dyn Future<Output = Option<FileHandle>> + Send>>;

enum Dialog {
    Open(Chosen),
    Save(Chosen, Vec<u8>),
}

struct Parent(RawWindowHandle);

impl Parent {
    fn of(window: &impl HasRawWindowHandle) -> Option<Parent> {
        match window.raw_window_handle() {
            raw_window_handle_05::RawWindowHandle::AppKit(handle) => NonNull::new(handle.ns_view).map(|view| Parent(RawWindowHandle::AppKit(AppKitWindowHandle::new(view)))),
            _ => None,
        }
    }
}

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(self.0) })
    }
}

impl HasDisplayHandle for Parent {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())) })
    }
}

fn dialog(window: &impl HasRawWindowHandle) -> AsyncFileDialog {
    match Parent::of(window) {
        Some(parent) => AsyncFileDialog::new().set_parent(&parent),
        None => AsyncFileDialog::new(),
    }
}

fn settle(dialog: &mut Dialog, editor: &nih_plug_webview::WindowHandler) -> bool {
    let chosen = match dialog {
        Dialog::Open(chosen) | Dialog::Save(chosen, _) => chosen,
    };
    let Poll::Ready(file) = chosen.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
        return false;
    };
    let Some(file) = file else {
        return true;
    };
    let path = file.path();
    match dialog {
        Dialog::Open(_) => match std::fs::read_to_string(path) {
            Ok(text) => editor.send_json(json!({ "type": "opened", "text": text })),
            Err(error) => log(&format!("open {}: {error}", path.display())),
        },
        Dialog::Save(_, bytes) => {
            if let Err(error) = std::fs::write(path, bytes) {
                log(&format!("save {}: {error}", path.display()));
            }
        }
    }
    true
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
                PAGE_SOURCES.get_file(rest).map(|f| f.contents())
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
    const VENDOR: &'static str = "JS";
    const URL: &'static str = "https://janne-s.github.io/Neartect/";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { aux_inputs: &["Sidechain"], ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { aux_inputs: &["Sidechain"], ..PortNames::const_default() },
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
        self.offline = buffer_config.process_mode == ProcessMode::Offline;
        self.main_channels = layout.main_input_channels.map_or(2, |n| n.get() as usize);
        self.sample_rate.store(rate.to_bits(), Ordering::Relaxed);
        self.convolvers = (0..SOURCES).map(|_| Convolver::new(rate)).collect();
        self.dry = [vec![0.0; BLOCK], vec![0.0; BLOCK]];
        let stored = self.params.filter.read().ok().and_then(|f| f.bank(rate));
        if let Some(bank) = stored {
            self.offer(bank, true);
            self.applied = self.builder.latest();
        } else {
            let space = self.params.live_space();
            if !space.is_empty() {
                self.builder.request(space, Some(rate as f64), None);
            }
        }
        context.set_latency_samples(BLOCK as u32);
        true
    }

    fn reset(&mut self) {
        let rate = f32::from_bits(self.sample_rate.load(Ordering::Relaxed));
        self.convolvers = (0..SOURCES).map(|_| Convolver::new(rate)).collect();
        if let Some(bank) = self.params.filter.read().ok().and_then(|f| f.bank(rate)) {
            self.offer(bank, true);
        }
        self.dry.iter_mut().for_each(|d| d.fill(0.0));
        self.dry_at = 0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.take_delivery(self.offline);
        for convolver in &mut self.convolvers {
            while let Some(old) = convolver.take_retired() {
                if let Err(error) = self.retired_out.try_send(Retired::Filter(old)) {
                    drop(error.into_inner());
                }
            }
        }
        if let Some(next) = self.signal_change.try_lock().ok().and_then(|mut change| change.take()) {
            self.phase = 0.0;
            if let Some(old) = std::mem::replace(&mut self.signal, next) {
                if let Err(error) = self.retired_out.try_send(Retired::Signal(old)) {
                    drop(error.into_inner());
                }
            }
        }
        self.clock.fetch_add(buffer.samples() as u64, Ordering::Relaxed);
        self.params.slots.moves.next_block();
        let stereo_input = self.main_channels > 1;
        let side = aux.inputs.first().map(|b| b.as_slice_immutable());
        let rate = f32::from_bits(self.sample_rate.load(Ordering::Relaxed));
        let step = 1.0 / (BYPASS_SECONDS * rate);
        let target = if self.params.bypass.value() { 0.0 } else { 1.0 };
        for (index, mut frame) in buffer.iter_samples().enumerate() {
            self.heard += (target - self.heard).clamp(-step, step);
            let mix = self.params.mix.smoothed.next() * self.heard;
            let gain = self.params.output.smoothed.next();
            let (left, right) = if let Some(signal) = &self.signal {
                let sample = SIGNAL_GAIN * signal.samples[self.phase as usize];
                self.phase += (signal.sample_rate / rate) as f64;
                if self.phase >= signal.samples.len() as f64 {
                    self.phase %= signal.samples.len() as f64;
                }
                (sample, sample)
            } else {
                let left = *frame.get_mut(0).expect("a main channel");
                let right = if stereo_input { *frame.get_mut(1).expect("a second channel") } else { left };
                (left, right)
            };
            let sided = side.map_or([0.0; 2], |s| [s.first().map_or(0.0, |c| c[index]), s.get(1).or(s.first()).map_or(0.0, |c| c[index])]);
            let mut wet = [0.0f32; 2];
            for (convolver, input) in self.convolvers.iter_mut().zip(self.inputs) {
                let out = convolver.process(input.pick([left, right], sided));
                wet[0] += out[0];
                wet[1] += out[1];
            }
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
        let builder = self.builder.clone();
        let page_in = self.page_in.clone();
        let page_out = self.page_out.clone();
        let editor_open = self.editor_open.clone();
        let (clock, audition, signal_change) = (self.clock.clone(), self.audition.clone(), self.signal_change.clone());
        let pending: Mutex<Option<Dialog>> = Mutex::new(None);
        let held: Mutex<(HashSet<usize>, Vec<f64>)> = Mutex::new((HashSet::new(), Vec::new()));
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
                while let Ok(outcome) = page_in.try_recv() {
                    if let Some(message) = outcome_json(&outcome) {
                        window.send_json(message);
                    }
                }
                let Ok(mut pending) = pending.lock() else {
                    return;
                };
                if pending.as_mut().is_some_and(|dialog| settle(dialog, window)) {
                    *pending = None;
                }
                if let Ok(mut held) = held.lock() {
                    if !held.1.is_empty() {
                        let mut changed = serde_json::Map::new();
                        for (index, entry) in params.slots.entries.iter().enumerate() {
                            let now = entry.plain();
                            if !held.0.contains(&index) && held.1[index] != now {
                                held.1[index] = now;
                                changed.insert(params.slots.specs[index].id.clone(), json!(now));
                            }
                        }
                        if !changed.is_empty() {
                            window.send_json(json!({ "type": "params", "values": changed }));
                        }
                    }
                }
                if params.scene_changed.swap(false, Ordering::AcqRel) && editor_open.load(Ordering::Acquire) {
                    window.send_json(json!({ "type": "scene", "space": params.live_document() }));
                }
                if params.bypass_changed.swap(false, Ordering::Relaxed) {
                    window.send_json(json!({ "type": "bypass", "on": params.bypass.value() }));
                }
                while let Ok(value) = window.next_event() {
                    match serde_json::from_value::<Message>(value) {
                        Ok(Message::Ready) => {
                            editor_open.store(true, Ordering::Release);
                            params.scene_changed.store(false, Ordering::Release);
                            let space = params.live_document();
                            let rate = f32::from_bits(sample_rate.load(Ordering::Relaxed));
                            let values = params.slots.values();
                            if let Ok(mut held) = held.lock() {
                                held.1 = values.clone();
                            }
                            let ids: Vec<&str> = params.slots.specs.iter().map(|s| s.id.as_str()).collect();
                            window.send_json(json!({
                                "type": "init", "sampleRateHz": rate, "bypass": params.bypass.value(), "synced": params.synced.read().is_ok_and(|s| *s),
                                "space": if space.is_empty() { None } else { Some(space) },
                                "seconds": clock.load(Ordering::Relaxed) as f64 / rate as f64,
                                "audition": audition.lock().map(|a| json!({ "signal": a.signal, "file": a.file, "playing": a.playing })).unwrap_or_default(),
                                "camera": params.camera.read().map(|c| c.clone()).unwrap_or_default(),
                                "parameters": params::specs_json(&params.slots.specs), "values": ids.iter().zip(&values).map(|(id, v)| (id.to_string(), json!(v))).collect::<serde_json::Map<_, _>>()
                            }));
                        }
                        Ok(Message::Set { id, value }) => {
                            let Some(index) = params.slots.index_of(&id) else { continue };
                            let ptr = params.slots.ptr(index);
                            let Ok(mut held) = held.lock() else { continue };
                            unsafe {
                                if held.0.insert(index) {
                                    setter.raw_context.raw_begin_set_parameter(ptr);
                                }
                                setter.raw_context.raw_set_parameter_normalized(ptr, ptr.preview_normalized(value as f32));
                            }
                            if let Some(known) = held.1.get_mut(index) {
                                *known = params.slots.entries[index].plain();
                            }
                        }
                        Ok(Message::Scene { number }) => {
                            setter.begin_set_parameter(&params.scene);
                            setter.set_parameter(&params.scene, number.clamp(1, scenes::SCENES as i32));
                            setter.end_set_parameter(&params.scene);
                            params.scene_changed.store(false, Ordering::Release);
                            let _ = params.live();
                        }
                        Ok(Message::Synced) => {
                            if let Ok(mut synced) = params.synced.write() {
                                *synced = true;
                            }
                        }
                        Ok(Message::Audition { signal, file, playing, samples, rate }) => {
                            let host = f32::from_bits(sample_rate.load(Ordering::Relaxed));
                            let decoded = samples
                                .and_then(|text| base64::engine::general_purpose::STANDARD.decode(text).ok())
                                .map(|bytes| bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect::<Vec<f32>>())
                                .filter(|samples| !samples.is_empty())
                                .map(|samples| Arc::new(Signal { samples, sample_rate: rate.filter(|r| *r > 0.0).unwrap_or(host) }));
                            let Ok(mut audition) = audition.lock() else { continue };
                            if decoded.is_some() {
                                audition.samples = decoded;
                            }
                            audition.signal = signal;
                            audition.file = file;
                            audition.playing = playing;
                            let heard = if audition.playing { audition.samples.clone() } else { None };
                            if let Ok(mut change) = signal_change.lock() {
                                *change = Some(heard);
                            }
                        }
                        Ok(Message::Camera { camera }) => {
                            if let Ok(mut stored) = params.camera.write() {
                                *stored = camera;
                            }
                        }
                        Ok(Message::End) => {
                            let Ok(mut held) = held.lock() else { continue };
                            for index in held.0.drain().collect::<Vec<_>>() {
                                unsafe { setter.raw_context.raw_end_set_parameter(params.slots.ptr(index)) };
                            }
                        }
                        Ok(Message::Space { json }) => {
                            if let Ok(mut space) = params.space.write() {
                                *space = json;
                            }
                        }
                        Ok(Message::Compute { id, json, purpose }) => {
                            if purpose == "export" {
                                let page_out = page_out.clone();
                                compute::export(json, id, move |outcome| {
                                    let _ = page_out.send(outcome);
                                });
                            } else {
                                builder.request(json, None, Some(id));
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
                            if pending.is_none() {
                                *pending = Some(Dialog::Open(Box::pin(dialog(base).add_filter("Neartect space", &["json"]).pick_file())));
                            }
                        }
                        Ok(Message::Save { name, base64 }) => match base64::engine::general_purpose::STANDARD.decode(base64) {
                            Ok(bytes) => {
                                if pending.is_none() {
                                    *pending = Some(Dialog::Save(Box::pin(dialog(base).set_file_name(&name).save_file()), bytes));
                                }
                            }
                            Err(error) => log(&format!("save {name}: {error}")),
                        },
                        Err(error) => log(&format!("unreadable message: {error}")),
                    }
                }
            });
        Some(Box::new(WatchedEditor { inner: editor, open: self.editor_open.clone(), wake: self.wake.clone() }))
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
