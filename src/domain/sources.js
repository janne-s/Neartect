import { firstListener, pointPosition } from './objects.js';

export const MOST_SOURCES = 4;
export const SOURCE_INPUTS = Object.freeze({ main: 'Main', mainLeft: 'Main L', mainRight: 'Main R', side: 'Side', sideLeft: 'Side L', sideRight: 'Side R' });
export const SOURCE_LEVEL_DB = Object.freeze([-36, 12]);
const NEW_SOURCE_METRES = 1.5;

export function sourceEntries(space) {
  const more = (space.moreSources ?? []).map(source => ({ number: source.slot, source, arrival: source.arrival }));
  return [{ number: 1, source: space.source, arrival: space.arrival }, ...more].sort((a, b) => a.number - b.number);
}

export const sourceByNumber = (space, number) => sourceEntries(space).find(entry => entry.number === number) ?? null;

export const sourceKey = number => `source:${number}`;

export function fillSourceSlots(space) {
  const list = space.moreSources ?? [];
  const taken = new Set(list.map(s => s.slot).filter(n => Number.isInteger(n) && n >= 2 && n <= MOST_SOURCES));
  for (const source of list) {
    if (Number.isInteger(source.slot) && source.slot >= 2) continue;
    let slot = 2;
    while (taken.has(slot)) slot += 1;
    source.slot = slot;
    taken.add(slot);
  }
}

export function newSourcePlace(space, number) {
  const centre = firstListener(space) ? pointPosition(space, firstListener(space)) : [0, 0, 1.2];
  const turn = (number - 1) * Math.PI / 2 + Math.PI / 4;
  return [centre[0] + NEW_SOURCE_METRES * Math.sin(turn), centre[1] + NEW_SOURCE_METRES * Math.cos(turn), centre[2]];
}
