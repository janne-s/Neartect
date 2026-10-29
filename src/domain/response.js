function fft(re, im) {
  const n = re.length;
  for (let i = 1, j = 0; i < n; i += 1) {
    let bit = n >> 1;
    for (; j & bit; bit >>= 1) j ^= bit;
    j ^= bit;
    if (i < j) {
      [re[i], re[j]] = [re[j], re[i]];
      [im[i], im[j]] = [im[j], im[i]];
    }
  }
  for (let size = 2; size <= n; size <<= 1) {
    const angle = -2 * Math.PI / size;
    for (let start = 0; start < n; start += size) {
      for (let k = 0; k < size / 2; k += 1) {
        const wr = Math.cos(angle * k);
        const wi = Math.sin(angle * k);
        const a = start + k;
        const b = a + size / 2;
        const tr = re[b] * wr - im[b] * wi;
        const ti = re[b] * wi + im[b] * wr;
        re[b] = re[a] - tr;
        im[b] = im[a] - ti;
        re[a] += tr;
        im[a] += ti;
      }
    }
  }
}

export function magnitudeResponse(samples, sampleRateHz) {
  let n = 1;
  while (n < samples.length * 2) n <<= 1;
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
