import init, { coreVersion, nearFilterFromParts, nearSpectrumPart, settingNames, surfaceHeaderLength } from '../core/neartect_core.js';

const WASM_URL = new URL('../core/neartect_core_bg.wasm', import.meta.url);
let ready = null;

export function loadCore() {
  if (!ready) {
    const source = WASM_URL.protocol === 'file:'
      ? import('node:fs/promises').then(fs => fs.readFile(WASM_URL))
      : Promise.resolve(WASM_URL);
    ready = source.then(module_or_path => init({ module_or_path }));
  }
  return ready.then(() => ({ coreVersion, nearFilterFromParts, nearSpectrumPart, settingNames, surfaceHeaderLength }));
}
