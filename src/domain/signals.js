import { createRandom } from '../shared/domain/random.js';

export const SIGNAL_SECONDS = 4;
const CLICK_INTERVAL_SECONDS = 0.5;
const SWEEP_RANGE_HZ = [20, 20000];
const FADE_SECONDS = 0.05;
const PINK_SEED = 1;

export function pinkNoise(sampleRate, seconds = SIGNAL_SECONDS, seed = PINK_SEED) {
  const random = createRandom(seed);
  const samples = new Float32Array(Math.round(sampleRate * seconds));
  let b0 = 0, b1 = 0, b2 = 0, b3 = 0, b4 = 0, b5 = 0, b6 = 0;
  for (let i = 0; i < samples.length; i += 1) {
    const white = random() * 2 - 1;
    b0 = 0.99886 * b0 + white * 0.0555179;
    b1 = 0.99332 * b1 + white * 0.0750759;
    b2 = 0.969 * b2 + white * 0.153852;
    b3 = 0.8665 * b3 + white * 0.3104856;
    b4 = 0.55 * b4 + white * 0.5329522;
    b5 = -0.7616 * b5 - white * 0.016898;
    samples[i] = (b0 + b1 + b2 + b3 + b4 + b5 + b6 + white * 0.5362) * 0.11;
    b6 = white * 0.115926;
  }
  return samples;
}

export function clicks(sampleRate, seconds = SIGNAL_SECONDS) {
  const samples = new Float32Array(Math.round(sampleRate * seconds));
  for (let t = 0; t < seconds; t += CLICK_INTERVAL_SECONDS) samples[Math.round(t * sampleRate)] = 0.9;
  return samples;
}

export function sweep(sampleRate, seconds = SIGNAL_SECONDS) {
  const samples = new Float32Array(Math.round(sampleRate * seconds));
  const [low, high] = SWEEP_RANGE_HZ;
  const rate = Math.log(high / low);
  const fade = Math.round(FADE_SECONDS * sampleRate);
  for (let i = 0; i < samples.length; i += 1) {
    const t = i / sampleRate;
    const phase = 2 * Math.PI * low * seconds / rate * (Math.exp(t / seconds * rate) - 1);
    const envelope = Math.min(1, i / fade, (samples.length - 1 - i) / fade);
    samples[i] = 0.5 * envelope * Math.sin(phase);
  }
  return samples;
}

export const TEST_SIGNALS = Object.freeze({ pink: pinkNoise, clicks, sweep });
