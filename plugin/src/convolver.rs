use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const BLOCK: usize = 128;
const SIZE: usize = 2 * BLOCK;
const BINS: usize = BLOCK + 1;
const FADE_SAMPLES: usize = 16 * BLOCK;
const MOST_RETIRED: usize = 4;
pub const MOST_SECONDS: f32 = 2.0;

pub struct Filter {
    pub sample_rate: f32,
    partitions: Vec<[Vec<Complex32>; 2]>,
}

impl Filter {
    pub fn new(sample_rate: f32, left: &[f32], right: &[f32]) -> Filter {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(SIZE);
        let most = most_partitions(sample_rate);
        let length = left.len().max(right.len());
        let count = length.div_ceil(BLOCK).clamp(1, most);
        let spectrum = |side: &[f32], p: usize| {
            let mut frame = vec![0.0; SIZE];
            let start = (p * BLOCK).min(side.len());
            let end = ((p + 1) * BLOCK).min(side.len());
            frame[..end - start].copy_from_slice(&side[start..end]);
            let mut bins = forward.make_output_vec();
            forward
                .process(&mut frame, &mut bins)
                .expect("sizes match the plan");
            let scale = 1.0 / SIZE as f32;
            bins.iter_mut().for_each(|b| *b *= scale);
            bins
        };
        let partitions = (0..count)
            .map(|p| [spectrum(left, p), spectrum(right, p)])
            .collect();
        Filter {
            sample_rate,
            partitions,
        }
    }

    pub fn partitions(&self) -> usize {
        self.partitions.len()
    }
}

pub fn most_partitions(sample_rate: f32) -> usize {
    ((MOST_SECONDS * sample_rate) as usize).div_ceil(BLOCK)
}

pub struct Convolver {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    frame: Vec<f32>,
    work: Vec<f32>,
    history: Vec<Vec<Complex32>>,
    head: usize,
    sums: [Vec<Complex32>; 2],
    time: Vec<f32>,
    forward_scratch: Vec<Complex32>,
    inverse_scratch: Vec<Complex32>,
    input: Vec<f32>,
    output: [Vec<f32>; 2],
    faded: [Vec<f32>; 2],
    position: usize,
    quiet_blocks: usize,
    current: Option<Box<Filter>>,
    next: Option<Box<Filter>>,
    queued: Option<Box<Filter>>,
    fade: usize,
    #[allow(clippy::vec_box)]
    retired: Vec<Box<Filter>>,
}

impl Convolver {
    pub fn new(sample_rate: f32) -> Convolver {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(SIZE);
        let inverse = planner.plan_fft_inverse(SIZE);
        let forward_scratch = forward.make_scratch_vec();
        let inverse_scratch = inverse.make_scratch_vec();
        let zero = Complex32::new(0.0, 0.0);
        Convolver {
            forward,
            inverse,
            frame: vec![0.0; SIZE],
            work: vec![0.0; SIZE],
            history: vec![vec![zero; BINS]; most_partitions(sample_rate)],
            head: 0,
            sums: [vec![zero; BINS], vec![zero; BINS]],
            time: vec![0.0; SIZE],
            forward_scratch,
            inverse_scratch,
            input: vec![0.0; BLOCK],
            output: [vec![0.0; BLOCK], vec![0.0; BLOCK]],
            faded: [vec![0.0; BLOCK], vec![0.0; BLOCK]],
            position: 0,
            quiet_blocks: usize::MAX / 2,
            current: None,
            next: None,
            queued: None,
            fade: 0,
            retired: Vec::with_capacity(MOST_RETIRED),
        }
    }

    pub fn set_now(&mut self, filter: Box<Filter>) {
        self.current = Some(filter);
        self.next = None;
        self.queued = None;
        self.fade = 0;
    }

    pub fn offer(&mut self, filter: Box<Filter>) {
        if self.current.is_none() {
            self.current = Some(filter);
        } else if self.next.is_none() {
            self.next = Some(filter);
            self.fade = FADE_SAMPLES;
        } else if let Some(old) = self.queued.replace(filter) {
            self.retire(old);
        }
    }

    pub fn take_retired(&mut self) -> Option<Box<Filter>> {
        self.retired.pop()
    }

    fn retire(&mut self, filter: Box<Filter>) {
        if self.retired.len() < MOST_RETIRED {
            self.retired.push(filter);
        }
    }

    #[inline]
    pub fn process(&mut self, sample: f32) -> [f32; 2] {
        self.input[self.position] = sample;
        let out = [self.output[0][self.position], self.output[1][self.position]];
        self.position += 1;
        if self.position == BLOCK {
            self.position = 0;
            self.run_block();
        }
        out
    }

    fn run_block(&mut self) {
        let silent = self.input.iter().all(|&x| x == 0.0);
        self.quiet_blocks = if silent {
            self.quiet_blocks.saturating_add(1)
        } else {
            0
        };
        self.head = (self.head + 1) % self.history.len();
        let longest = self
            .current
            .as_ref()
            .map_or(0, |f| f.partitions())
            .max(self.next.as_ref().map_or(0, |f| f.partitions()));
        if self.quiet_blocks > longest {
            self.history[self.head].fill(Complex32::new(0.0, 0.0));
            self.frame.fill(0.0);
            self.output.iter_mut().for_each(|o| o.fill(0.0));
            self.step_fade();
            return;
        }
        self.frame.copy_within(BLOCK.., 0);
        self.frame[BLOCK..].copy_from_slice(&self.input);
        self.work.copy_from_slice(&self.frame);
        let head = self.head;
        self.forward
            .process_with_scratch(
                &mut self.work,
                &mut self.history[head],
                &mut self.forward_scratch,
            )
            .expect("sizes match the plan");

        if let Some(filter) = self.current.take() {
            let mut output = std::mem::take(&mut self.output);
            self.convolve(&filter, &mut output);
            self.output = output;
            self.current = Some(filter);
        } else {
            self.output.iter_mut().for_each(|o| o.fill(0.0));
        }
        if let Some(filter) = self.next.take() {
            let mut faded = std::mem::take(&mut self.faded);
            self.convolve(&filter, &mut faded);
            let done = FADE_SAMPLES - self.fade;
            for i in 0..BLOCK {
                let g = ((done + i) as f32 / FADE_SAMPLES as f32).min(1.0);
                for ear in 0..2 {
                    self.output[ear][i] = self.output[ear][i] * (1.0 - g) + faded[ear][i] * g;
                }
            }
            self.faded = faded;
            self.next = Some(filter);
        }
        self.step_fade();
    }

    fn step_fade(&mut self) {
        if self.next.is_none() {
            return;
        }
        self.fade = self.fade.saturating_sub(BLOCK);
        if self.fade == 0 {
            let old = std::mem::replace(&mut self.current, self.next.take());
            if let Some(old) = old {
                self.retire(old);
            }
            if let Some(queued) = self.queued.take() {
                self.next = Some(queued);
                self.fade = FADE_SAMPLES;
            }
        }
    }

    fn convolve(&mut self, filter: &Filter, output: &mut [Vec<f32>; 2]) {
        let slots = self.history.len();
        for ear in 0..2 {
            let sum = &mut self.sums[ear];
            sum.fill(Complex32::new(0.0, 0.0));
            for (p, partition) in filter.partitions.iter().enumerate() {
                let spectrum = &self.history[(self.head + slots - p) % slots];
                for ((s, x), h) in sum.iter_mut().zip(spectrum).zip(&partition[ear]) {
                    *s += x * h;
                }
            }
            sum[0].im = 0.0;
            sum[BINS - 1].im = 0.0;
            self.inverse
                .process_with_scratch(sum, &mut self.time, &mut self.inverse_scratch)
                .expect("sizes match the plan");
            output[ear].copy_from_slice(&self.time[BLOCK..]);
        }
    }
}
