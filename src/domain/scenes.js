import { readSpace, SPACE_KIND } from './space.js';

export const SCENES = 8;
export const SCENE_NAME_LENGTH = 24;

const isRecord = value => value !== null && typeof value === 'object' && !Array.isArray(value);

export function sceneNumber(value) {
  const n = Number(value);
  return Number.isFinite(n) ? Math.min(SCENES, Math.max(1, Math.round(n))) : 1;
}

export function sceneName(value) {
  if (typeof value !== 'string') return null;
  const name = value.trim().slice(0, SCENE_NAME_LENGTH);
  return name || null;
}

export function resolveScene(list, number) {
  const n = sceneNumber(number);
  for (let i = n; i >= 1; i -= 1) if (list[i - 1]) return i;
  for (let i = n + 1; i <= SCENES; i += 1) if (list[i - 1]) return i;
  return 1;
}

export function readScenes(value) {
  const active = readSpace(value);
  const scene = sceneNumber(value.scene);
  const listed = Array.isArray(value.scenes) ? value.scenes : [];
  const names = Array.isArray(value.sceneNames) ? value.sceneNames : [];
  const list = Array.from({ length: SCENES }, (_, i) => {
    if (i === scene - 1) return active;
    if (!isRecord(listed[i])) return null;
    try {
      return readSpace({ ...listed[i], kind: SPACE_KIND });
    } catch {
      return null;
    }
  });
  return { list, scene, names: Array.from({ length: SCENES }, (_, i) => (list[i] ? sceneName(names[i]) : null)) };
}

export function sceneDocument(space, { list, scene, names }) {
  const scenes = list.map((item, i) => (i === scene - 1 ? space : item ?? null));
  if (scene === 1 && scenes.filter(Boolean).length === 1 && !names[0]) return space;
  const document = { ...space, scene, scenes };
  if (names.some(Boolean)) document.sceneNames = names.map((name, i) => (scenes[i] ? name : null));
  return document;
}
