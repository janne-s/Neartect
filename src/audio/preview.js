const CROSSFADE_SECONDS = 0.04;
const OUTPUT_GAIN = 0.35;

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
    branches = [0, 1].map(() => {
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

  function levels(time) {
    branches.forEach((branch, index) => {
      branch.gain.gain.cancelScheduledValues(time);
      branch.gain.gain.setTargetAtTime(!bypass && index === active && branch.convolver.buffer ? 1 : 0, time, CROSSFADE_SECONDS / 3);
    });
    dry.gain.cancelScheduledValues(time);
    dry.gain.setTargetAtTime(bypass ? 1 : 0, time, CROSSFADE_SECONDS / 3);
  }

  function setFilter(filter) {
    pending = filter;
    if (!context) return;
    const sides = stereo(filter.channels, filter.pans);
    const buffer = context.createBuffer(sides.length, sides[0].length, filter.sampleRateHz);
    sides.forEach((side, index) => buffer.copyToChannel(side, index));
    active = 1 - active;
    branches[active].convolver.buffer = buffer;
    levels(context.currentTime);
  }

  function connect(node) {
    branches.forEach(branch => node.connect(branch.convolver));
    node.connect(dry);
  }

  return {
    get sampleRate() { return ensure().sampleRate; },
    get playing() { return Boolean(source); },
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
