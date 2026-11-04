const CROSSFADE_SECONDS = 0.04;
const OUTPUT_GAIN = 0.35;
const BRANCHES = 4;
const RELEASE_MARGIN_SECONDS = 0.05;

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
      return { convolver, gain, silentFrom: Infinity };
    });
    dry = context.createGain();
    dry.gain.value = 0;
    dry.connect(output);
    if (pending) setFilter(pending);
    return context;
  }

  function ramp(param, target, time, fade) {
    param.cancelScheduledValues(time);
    param.setValueAtTime(param.value, time);
    param.linearRampToValueAtTime(target, time + fade);
  }

  // A convolver at zero gain still convolves.
  function levels(time, fade = CROSSFADE_SECONDS) {
    branches.forEach((branch, index) => {
      const heard = !bypass && index === active && branch.convolver.buffer;
      const silent = branch.gain.gain.value === 0;
      ramp(branch.gain.gain, heard ? 1 : 0, time, fade);
      branch.silentFrom = heard ? Infinity : silent ? Math.min(branch.silentFrom, time) : time + fade;
    });
    ramp(dry.gain, bypass ? 1 : 0, time, CROSSFADE_SECONDS);
    release();
  }

  function release() {
    let wait = Infinity;
    branches.forEach((branch, index) => {
      if (index === active || !branch.convolver.buffer) return;
      if (context.currentTime >= branch.silentFrom) branch.convolver.buffer = null;
      else wait = Math.min(wait, branch.silentFrom - context.currentTime);
    });
    if (wait < Infinity) setTimeout(release, (wait + RELEASE_MARGIN_SECONDS) * 1000);
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
    branches[active].silentFrom = Infinity;
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
      if (navigator.audioSession) navigator.audioSession.type = 'playback';
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
    silence() {
      pending = null;
      if (!context) return;
      branches.forEach(branch => ramp(branch.gain.gain, 0, context.currentTime, CROSSFADE_SECONDS));
    },
    setBypass(on) {
      bypass = on;
      if (context) levels(context.currentTime);
    },
    async decode(arrayBuffer) {
      return ensure().decodeAudioData(arrayBuffer);
    }
  };
}
