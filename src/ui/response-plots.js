import { fitCanvas, niceStep, palette } from '../shared/ui/canvas.js';
import { magnitudeResponse } from '../domain/response.js';

const PAD = { left: 34, right: 8, top: 8, bottom: 18 };
const LOWEST_HZ = 20;
const HIGHEST_HZ = 20000;
const DECIBEL_RANGE = [-30, 20];
const FREQUENCY_TICKS = [50, 100, 200, 500, 1000, 2000, 5000, 10000];

function frame(canvas) {
  const { context, width, height } = fitCanvas(canvas);
  const colors = palette();
  context.font = '10px system-ui, sans-serif';
  context.lineWidth = 1;
  const plot = { x: PAD.left, y: PAD.top, w: Math.max(1, width - PAD.left - PAD.right), h: Math.max(1, height - PAD.top - PAD.bottom) };
  return { context, colors, plot };
}

export function drawImpulse(canvas, filter) {
  const { context, colors, plot } = frame(canvas);
  if (!filter) return;
  const { samples, sampleRateHz } = filter;
  let peak = 1e-9;
  for (const x of samples) peak = Math.max(peak, Math.abs(x));
  const seconds = samples.length / sampleRateHz;
  const step = niceStep(seconds * 1000, 6);
  context.strokeStyle = colors.grid;
  context.fillStyle = colors.faint;
  for (let ms = 0; ms <= seconds * 1000; ms += step) {
    const x = plot.x + plot.w * ms / (seconds * 1000);
    context.beginPath();
    context.moveTo(x, plot.y);
    context.lineTo(x, plot.y + plot.h);
    context.stroke();
    context.fillText(`${+ms.toFixed(1)} ms`, x + 2, plot.y + plot.h + 12);
  }
  const middle = plot.y + plot.h / 2;
  context.beginPath();
  context.moveTo(plot.x, middle);
  context.lineTo(plot.x + plot.w, middle);
  context.stroke();
  context.strokeStyle = colors.accent;
  context.beginPath();
  const columns = Math.ceil(plot.w);
  for (let column = 0; column < columns; column += 1) {
    const a = Math.floor(column / columns * samples.length);
    const b = Math.max(a + 1, Math.floor((column + 1) / columns * samples.length));
    let lo = 0;
    let hi = 0;
    for (let i = a; i < b; i += 1) { lo = Math.min(lo, samples[i]); hi = Math.max(hi, samples[i]); }
    const x = plot.x + column;
    context.moveTo(x, middle - hi / peak * plot.h / 2);
    context.lineTo(x, middle - lo / peak * plot.h / 2 + 0.5);
  }
  context.stroke();
}

export function drawResponse(canvas, filter) {
  const { context, colors, plot } = frame(canvas);
  const xOf = hz => plot.x + plot.w * Math.log(hz / LOWEST_HZ) / Math.log(HIGHEST_HZ / LOWEST_HZ);
  const yOf = db => plot.y + plot.h * (DECIBEL_RANGE[1] - db) / (DECIBEL_RANGE[1] - DECIBEL_RANGE[0]);
  context.strokeStyle = colors.grid;
  context.fillStyle = colors.faint;
  for (const hz of FREQUENCY_TICKS) {
    const x = xOf(hz);
    context.beginPath();
    context.moveTo(x, plot.y);
    context.lineTo(x, plot.y + plot.h);
    context.stroke();
    context.fillText(hz >= 1000 ? `${hz / 1000}k` : `${hz}`, x + 2, plot.y + plot.h + 12);
  }
  for (let db = DECIBEL_RANGE[0]; db <= DECIBEL_RANGE[1]; db += 10) {
    const y = yOf(db);
    context.strokeStyle = db === 0 ? colors.muted : colors.grid;
    context.beginPath();
    context.moveTo(plot.x, y);
    context.lineTo(plot.x + plot.w, y);
    context.stroke();
    context.fillText(`${db > 0 ? '+' : ''}${db}`, 4, y + 3);
  }
  if (!filter) return;
  const { frequencies, decibels } = magnitudeResponse(filter.samples, filter.sampleRateHz);
  context.strokeStyle = colors.accent;
  context.lineWidth = 1.5;
  context.beginPath();
  let started = false;
  for (let k = 1; k < frequencies.length; k += 1) {
    const hz = frequencies[k];
    if (hz < LOWEST_HZ || hz > HIGHEST_HZ) continue;
    const y = yOf(Math.min(DECIBEL_RANGE[1] + 5, Math.max(DECIBEL_RANGE[0] - 5, decibels[k])));
    if (started) context.lineTo(xOf(hz), y); else { context.moveTo(xOf(hz), y); started = true; }
  }
  context.stroke();
}
