const CROSSFADE_SECONDS = 0.04;
const OUTPUT_GAIN = 0.35;
const BRANCHES = 4;

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
  let output = null;
  let dry = null;
  let bypass = false;
  let pending = null;
  const tracks = new Map();
  let playing = false;

  function ensure() {
    if (context) return context;
    context = new AudioContext();
    output = context.createGain();
    output.gain.value = OUTPUT_GAIN;
    output.connect(context.destination);
    dry = context.createGain();
    dry.gain.value = 0;
    dry.connect(output);
    if (pending) setFilter(pending);
    return context;
  }

  function track(number) {
    if (!tracks.has(number)) {
      const branches = Array.from({ length: BRANCHES }, () => {
        const convolver = context.createConvolver();
        convolver.normalize = false;
        const gain = context.createGain();
        gain.gain.value = 0;
        convolver.connect(gain).connect(output);
        return { convolver, gain, silentFrom: Infinity, clock: null, releaseTime: Infinity };
      });
      tracks.set(number, { branches, active: 0, node: null, heard: false });
    }
    return tracks.get(number);
  }

  function ramp(param, target, time, fade) {
    param.cancelScheduledValues(time);
    param.setValueAtTime(param.value, time);
    param.linearRampToValueAtTime(target, time + fade);
  }

  // A convolver at zero gain still convolves.
  function levels(time, fade = CROSSFADE_SECONDS) {
    for (const t of tracks.values()) {
      t.branches.forEach((branch, index) => {
        const heard = !bypass && t.heard && index === t.active && branch.convolver.buffer;
        const silent = branch.gain.gain.value === 0;
        if (heard && branch.clock) {
          const clock = branch.clock;
          branch.clock = null;
          clock.stop();
        }
        ramp(branch.gain.gain, heard ? 1 : 0, time, fade);
        branch.silentFrom = heard ? Infinity : silent ? Math.min(branch.silentFrom, time) : time + fade;
      });
      release(t);
    }
    ramp(dry.gain, bypass ? 1 : 0, time, CROSSFADE_SECONDS);
  }

  function release(t) {
    t.branches.forEach((branch, index) => {
      if ((index === t.active && t.heard) || !branch.convolver.buffer) return;
      if (context.currentTime >= branch.silentFrom) branch.convolver.buffer = null;
      else releaseAt(t, branch, branch.silentFrom);
    });
  }

  function releaseAt(t, branch, time) {
    if (branch.clock && branch.releaseTime === time) return;
    const clock = context.createConstantSource();
    clock.offset.value = 0;
    clock.connect(output);
    clock.onended = () => {
      clock.disconnect();
      if (branch.clock !== clock) return;
      branch.clock = null;
      if (t.branches.indexOf(branch) !== t.active || !t.heard) branch.convolver.buffer = null;
    };
    const previous = branch.clock;
    branch.clock = clock;
    branch.releaseTime = time;
    previous?.stop();
    clock.start();
    clock.stop(time);
  }

  function setFilter(filter, fade = CROSSFADE_SECONDS) {
    pending = filter;
    if (!context || filter.sampleRateHz !== context.sampleRate) return;
    const numbers = new Set();
    for (const source of filter.sources) {
      numbers.add(source.number);
      const t = track(source.number);
      const sides = stereo(source.channels, source.pans);
      const buffer = context.createBuffer(sides.length, sides[0].length, filter.sampleRateHz);
      sides.forEach((side, index) => buffer.copyToChannel(side, index));
      t.active = (t.active + 1) % BRANCHES;
      const branch = t.branches[t.active];
      branch.gain.gain.cancelScheduledValues(context.currentTime);
      branch.gain.gain.setValueAtTime(0, context.currentTime);
      branch.convolver.buffer = buffer;
      branch.silentFrom = Infinity;
      t.heard = true;
    }
    for (const [number, t] of tracks) if (!numbers.has(number)) t.heard = false;
    levels(context.currentTime, Math.max(CROSSFADE_SECONDS, fade));
  }

  function stop() {
    for (const t of tracks.values()) {
      if (!t.node) continue;
      t.node.stop();
      t.node.disconnect();
      t.node = null;
    }
    playing = false;
  }

  return {
    get sampleRate() { return ensure().sampleRate; },
    get playing() { return playing; },
    get time() { return context ? context.currentTime : 0; },
    async play(signals) {
      if (navigator.audioSession) navigator.audioSession.type = 'playback';
      const audio = ensure();
      await audio.resume();
      stop();
      for (const { number, samples, sampleRate } of signals) {
        const buffer = audio.createBuffer(1, samples.length, sampleRate);
        buffer.copyToChannel(samples, 0);
        const node = audio.createBufferSource();
        node.buffer = buffer;
        node.loop = true;
        const t = track(number);
        t.branches.forEach(branch => node.connect(branch.convolver));
        node.connect(dry);
        t.node = node;
      }
      playing = true;
      levels(audio.currentTime);
      for (const t of tracks.values()) t.node?.start();
    },
    stop,
    setFilter,
    silence() {
      pending = null;
      if (!context) return;
      for (const t of tracks.values()) {
        t.heard = false;
        t.branches.forEach(branch => ramp(branch.gain.gain, 0, context.currentTime, CROSSFADE_SECONDS));
      }
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
