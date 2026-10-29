import { fft, nextPowerOfTwo } from '../shared/domain/fft.js';

export function magnitudeResponse(samples, sampleRateHz) {
  const n = nextPowerOfTwo(samples.length * 2);
  const re = new Float64Array(n);
  const im = new Float64Array(n);
  re.set(samples);
  fft(re, im);
  const half = n / 2;
  const frequencies = new Float64Array(half + 1);
  const decibels = new Float64Array(half + 1);
  for (let k = 0; k <= half; k += 1) {
    frequencies[k] = k * sampleRateHz / n;
    decibels[k] = 20 * Math.log10(Math.max(1e-6, Math.hypot(re[k], im[k])));
  }
  return { frequencies, decibels };
}
