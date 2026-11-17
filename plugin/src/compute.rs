use crossbeam_channel::{select, unbounded, Receiver, Sender};
use neartect_core::{jobs, space};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

const MOST_THREADS: usize = 6;
const CACHED_FILTERS: usize = 8;

type Task = Box<dyn FnOnce() + Send>;

fn pool() -> &'static Sender<Task> {
    static POOL: OnceLock<Sender<Task>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tasks, queue) = unbounded::<Task>();
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).saturating_sub(1).clamp(1, MOST_THREADS);
        for index in 0..threads {
            let queue = queue.clone();
            let _ = std::thread::Builder::new().name(format!("neartect-filter-{index}")).spawn(move || {
                while let Ok(task) = queue.recv() {
                    task();
                }
            });
        }
        tasks
    })
}

pub struct SourceFilter {
    pub number: usize,
    pub name: String,
    pub input: &'static str,
    pub first: usize,
    pub count: usize,
    pub stereo: [Vec<f32>; 2],
}

pub struct Computed {
    pub sample_rate_hz: f64,
    pub channels: Vec<Vec<f32>>,
    pub names: Vec<String>,
    pub pans: Vec<[f64; 2]>,
    pub sources: Vec<SourceFilter>,
    pub moving: bool,
}

type Cache = Mutex<Vec<(u64, Arc<Computed>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

fn cached(key: u64) -> Option<Arc<Computed>> {
    let mut list = cache().lock().ok()?;
    let at = list.iter().position(|(k, _)| *k == key)?;
    let entry = list.remove(at);
    let computed = entry.1.clone();
    list.push(entry);
    Some(computed)
}

fn remember(key: u64, computed: Arc<Computed>) {
    if let Ok(mut list) = cache().lock() {
        list.retain(|(k, _)| *k != key);
        list.push((key, computed));
        let extra = list.len().saturating_sub(CACHED_FILTERS);
        list.drain(..extra);
    }
}

pub type Still = Arc<dyn Fn() -> bool + Send + Sync>;

pub fn compute(json: &str, rate: Option<f64>, seconds: Option<f64>, still: Still) -> Result<Option<Arc<Computed>>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| "This is not a Neartect space.".to_string())?;
    let mut space = space::read_space(&value).map_err(|e| e.0.to_string())?;
    if let Some(rate) = rate {
        space.render.sample_rate_hz = rate;
    }
    let moving = seconds.is_some() && space::moving(&space);
    if let Some(seconds) = seconds.filter(|_| moving) {
        space::set_in_motion(&mut space, seconds);
    }
    let mut hasher = DefaultHasher::new();
    space::space_json(&space).to_string().hash(&mut hasher);
    let key = hasher.finish();
    if let Some(computed) = cached(key) {
        return Ok(Some(computed));
    }
    let alone = jobs::source_spaces(&space);
    let plans: Vec<Arc<jobs::Plan>> = alone.iter().map(|s| jobs::plan(&s.space).map(Arc::new)).collect::<Result<_, _>>()?;
    let (done, results) = unbounded();
    let mut count = 0;
    for (s, plan) in plans.iter().enumerate() {
        for (c, channel) in plan.channels.iter().enumerate() {
            for p in 0..channel.pieces.len() {
                let (plan, done, still) = (plan.clone(), done.clone(), still.clone());
                count += 1;
                let _ = pool().send(Box::new(move || {
                    let spectra = still().then(|| jobs::spectra_of(&plan.channels[c].pieces[p]));
                    let _ = done.send((s, c, p, spectra));
                }));
            }
        }
    }
    drop(done);
    let mut spectra: Vec<Vec<Vec<Option<_>>>> = plans.iter().map(|plan| plan.channels.iter().map(|c| (0..c.pieces.len()).map(|_| None).collect()).collect()).collect();
    for _ in 0..count {
        let Ok((s, c, p, piece)) = results.recv() else { return Ok(None) };
        let Some(piece) = piece else { return Ok(None) };
        spectra[s][c][p] = Some(piece);
    }
    let (mut channels, mut names, mut pans, mut sources) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for ((meta, plan), pieces) in alone.iter().zip(&plans).zip(spectra) {
        let own: Vec<Vec<f32>> = plan.channels.iter().zip(pieces).flat_map(|(channel, pieces)| jobs::finish(channel, &pieces.into_iter().flatten().collect::<Vec<_>>())).collect();
        let stereo = if own.is_empty() { [vec![0.0], vec![0.0]] } else { jobs::stereo(&own, &plan.pans) };
        sources.push(SourceFilter { number: meta.number, name: meta.name.clone(), input: meta.input, first: channels.len(), count: own.len(), stereo });
        channels.extend(own);
        names.extend(plan.names.iter().cloned());
        pans.extend(plan.pans.iter().copied());
    }
    let computed = Arc::new(Computed {
        sample_rate_hz: space.render.sample_rate_hz,
        channels,
        names,
        pans,
        sources,
        moving,
    });
    remember(key, computed.clone());
    Ok(Some(computed))
}

pub struct Request {
    pub generation: u64,
    pub json: String,
    pub rate: Option<f64>,
    pub page: Option<u64>,
}

pub enum Outcome {
    Built {
        generation: u64,
        page: Option<u64>,
        computed: Arc<Computed>,
        milliseconds: f64,
    },
    Cancelled {
        generation: u64,
        page: Option<u64>,
    },
    Failed {
        generation: u64,
        page: Option<u64>,
        message: String,
    },
}

enum Event<T> {
    Retired(T),
    Woken,
    Asked(Request),
    Rolled,
    Closed,
}

pub struct Builder {
    latest: Arc<AtomicU64>,
    required: Arc<AtomicU64>,
    requests: Sender<Request>,
}

impl Builder {
    pub fn new<T: Send + 'static>(
        retired: Receiver<T>,
        wake: Receiver<()>,
        source: impl Fn() -> Option<(String, Option<f64>)> + Send + 'static,
        clock: impl Fn() -> f64 + Send + 'static,
        deliver: impl Fn(Outcome) + Send + Sync + 'static,
    ) -> Builder {
        let latest = Arc::new(AtomicU64::new(0));
        let required = Arc::new(AtomicU64::new(0));
        let (requests, queue) = unbounded::<Request>();
        let current = latest.clone();
        let deliver = Arc::new(deliver);
        let cancel = deliver.clone();
        let build = move |request: &Request| -> bool {
            let generation = request.generation;
            let watched = current.clone();
            let still: Still = Arc::new(move || watched.load(Ordering::Acquire) == generation);
            let started = Instant::now();
            let (outcome, moving) = match compute(&request.json, request.rate, Some(clock()), still) {
                Ok(Some(computed)) => {
                    let moving = computed.moving;
                    (Outcome::Built { generation, page: request.page, computed, milliseconds: started.elapsed().as_secs_f64() * 1000.0 }, moving)
                }
                Ok(None) => (Outcome::Cancelled { generation, page: request.page }, true),
                Err(message) => (Outcome::Failed { generation, page: request.page, message }, false),
            };
            deliver(outcome);
            moving
        };
        let (next, needed) = (latest.clone(), required.clone());
        let _ = std::thread::Builder::new().name("neartect-builder".into()).spawn(move || {
            let mut base: Option<Request> = None;
            let mut rolling = false;
            loop {
                let event = if rolling {
                    select! {
                        recv(retired) -> old => old.map_or(Event::Closed, Event::Retired),
                        recv(wake) -> woken => woken.map_or(Event::Closed, |_| Event::Woken),
                        recv(queue) -> request => request.map_or(Event::Closed, Event::Asked),
                        default => Event::Rolled,
                    }
                } else {
                    select! {
                        recv(retired) -> old => old.map_or(Event::Closed, Event::Retired),
                        recv(wake) -> woken => woken.map_or(Event::Closed, |_| Event::Woken),
                        recv(queue) -> request => request.map_or(Event::Closed, Event::Asked),
                    }
                };
                let rolled = matches!(event, Event::Rolled);
                let request = match event {
                    Event::Closed => return,
                    Event::Retired(old) => {
                        drop(old);
                        continue;
                    }
                    Event::Woken => {
                        let Some((json, rate)) = source() else { continue };
                        let generation = next.fetch_add(1, Ordering::AcqRel) + 1;
                        needed.store(generation, Ordering::Release);
                        Request { generation, json, rate, page: None }
                    }
                    Event::Asked(mut request) => {
                        while let Ok(newer) = queue.try_recv() {
                            cancel(Outcome::Cancelled { generation: request.generation, page: request.page });
                            request = newer;
                        }
                        request
                    }
                    Event::Rolled => {
                        let Some(base) = &base else {
                            rolling = false;
                            continue;
                        };
                        let generation = next.fetch_add(1, Ordering::AcqRel) + 1;
                        Request { generation, json: base.json.clone(), rate: base.rate, page: None }
                    }
                };
                rolling = build(&request);
                if !rolled {
                    base = Some(Request { page: None, ..request });
                }
            }
        });
        Builder { latest, required, requests }
    }

    pub fn request(&self, json: String, rate: Option<f64>, page: Option<u64>) -> u64 {
        let generation = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
        self.required.store(generation, Ordering::Release);
        let _ = self.requests.send(Request { generation, json, rate, page });
        generation
    }

    pub fn latest(&self) -> u64 {
        self.required.load(Ordering::Acquire)
    }
}

pub fn export(json: String, page: u64, deliver: impl FnOnce(Outcome) + Send + 'static) {
    let _ = std::thread::Builder::new().name("neartect-export".into()).spawn(move || {
        let started = Instant::now();
        deliver(match compute(&json, None, None, Arc::new(|| true)) {
            Ok(Some(computed)) => Outcome::Built {
                generation: 0,
                page: Some(page),
                computed,
                milliseconds: started.elapsed().as_secs_f64() * 1000.0,
            },
            Ok(None) => Outcome::Cancelled { generation: 0, page: Some(page) },
            Err(message) => Outcome::Failed {
                generation: 0,
                page: Some(page),
                message,
            },
        });
    });
}
