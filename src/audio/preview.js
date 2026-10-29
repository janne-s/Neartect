const CROSSFADE_SECONDS = 0.04;
const OUTPUT_GAIN = 0.35;
const BRANCHES = 4;

// Each channel plays at its [left, right] gains: free microphones spread from
// left to right, stereo pairs sit at their sides, and mid-side decodes.
export function stereo(channels, pans) {
  const length = channels[0].length;
  const left = new Float32Array(length);
  const right = new Float32Array(length);
  channels.forEach((channel, index) => {
    const [l, r] = pans[index];
    for (let i = 0; i < length; i += 1) {
      left[i] += l * channel[i];
      right[i] += r * channel[i];
    }
  });
  return [left, right];
}

export function createPreview() {
  let context = null;
  let source = null;
  let signal = null;
  let branches = [];
  let active = 0;
  let dry = null;
  let bypass = false;
  let pending = null;

  function ensure() {
    if (context) return context;
    context = new AudioContext();
    const output = context.createGain();
    output.gain.value = OUTPUT_GAIN;
    output.connect(context.destination);
    branches = Array.from({ length: BRANCHES }, () => {
      const convolver = context.createConvolver();
      convolver.normalize = false;
      const gain = context.createGain();
      gain.gain.value = 0;
      convolver.connect(gain).connect(output);
      return { convolver, gain };
    });
    dry = context.createGain();
    dry.gain.value = 0;
    dry.connect(output);
    if (pending) setFilter(pending);
    return context;
  }

  // Consecutive filters glide: the new one rises and the others fall in
  // straight lines over the fade, which is a linear interpolation between
  // the two filters while it lasts. Moving relief passes the time between
  // updates as the fade, so the sound slides from one filter to the next.
  function ramp(param, target, time, fade) {
    param.cancelScheduledValues(time);
    param.setValueAtTime(param.value, time);
    param.linearRampToValueAtTime(target, time + fade);
  }

  function levels(time, fade = CROSSFADE_SECONDS) {
    branches.forEach((branch, index) => ramp(branch.gain.gain, !bypass && index === active && branch.convolver.buffer ? 1 : 0, time, fade));
    ramp(dry.gain, bypass ? 1 : 0, time, CROSSFADE_SECONDS);
  }

  function setFilter(filter, fade = CROSSFADE_SECONDS) {
    pending = filter;
    if (!context || filter.sampleRateHz !== context.sampleRate) return;
    const sides = stereo(filter.channels, filter.pans);
    const buffer = context.createBuffer(sides.length, sides[0].length, filter.sampleRateHz);
    sides.forEach((side, index) => buffer.copyToChannel(side, index));
    active = (active + 1) % BRANCHES;
    branches[active].gain.gain.cancelScheduledValues(context.currentTime);
    branches[active].gain.gain.setValueAtTime(0, context.currentTime);
    branches[active].convolver.buffer = buffer;
    levels(context.currentTime, Math.max(CROSSFADE_SECONDS, fade));
  }

  function connect(node) {
    branches.forEach(branch => node.connect(branch.convolver));
    node.connect(dry);
  }

  return {
    get sampleRate() { return ensure().sampleRate; },
    get playing() { return Boolean(source); },
    get time() { return context ? context.currentTime : 0; },
    async play(samples, sampleRate) {
      const audio = ensure();
      await audio.resume();
      this.stop();
      signal = audio.createBuffer(1, samples.length, sampleRate);
      signal.copyToChannel(samples, 0);
      source = audio.createBufferSource();
      source.buffer = signal;
      source.loop = true;
      connect(source);
      levels(audio.currentTime);
      source.start();
    },
    stop() {
      if (!source) return;
      source.stop();
      source.disconnect();
      source = null;
    },
    setFilter,
    setBypass(on) {
      bypass = on;
      if (context) levels(context.currentTime);
    },
    async decode(arrayBuffer) {
      return ensure().decodeAudioData(arrayBuffer);
    }
  };
}
