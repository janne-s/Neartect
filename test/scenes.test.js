import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readScenes, resolveScene, sceneDocument, SCENES } from '../src/domain/scenes.js';
import { defaultSpace, SPACE_KIND } from '../src/domain/space.js';

const named = name => ({ kind: SPACE_KIND, objects: [{ kind: 'plane', id: name, name }] });

test('a space without scenes is scene one and saves as a plain space', () => {
  const read = readScenes(named('a'));
  assert.equal(read.scene, 1);
  assert.equal(read.list.filter(Boolean).length, 1);
  const saved = sceneDocument(read.list[0], read);
  assert.equal(saved.scenes, undefined);
  assert.equal(saved.scene, undefined);
});

test('the body is the active scene and broken or missing entries are empty', () => {
  const read = readScenes({ ...named('body'), scene: 3.4, scenes: [named('one'), 'x', named('stale'), null, { kind: 'other', objects: 7 }], sceneNames: ['  Dry  ', 'ghost', 'Wet', null, 5] });
  assert.equal(read.scene, 3);
  assert.equal(read.list.length, SCENES);
  assert.equal(read.list[0].objects[0].id, 'one');
  assert.equal(read.list[1], null);
  assert.equal(read.list[2].objects[0].id, 'body');
  assert.deepEqual(read.names.slice(0, 3), ['Dry', null, 'Wet']);
});

test('a saved document reads back to the same scenes', () => {
  const read = readScenes({ ...named('body'), scene: 2, scenes: [named('one')], sceneNames: ['First'] });
  const again = readScenes(JSON.parse(JSON.stringify(sceneDocument(read.list[1], read))));
  assert.equal(again.scene, 2);
  assert.equal(again.list[0].objects[0].id, 'one');
  assert.equal(again.list[1].objects[0].id, 'body');
  assert.equal(again.names[0], 'First');
});

test('an empty number falls to the nearest scene below, then above', () => {
  const list = [null, defaultSpace(), null, defaultSpace(), null, null, null, null];
  assert.deepEqual([1, 2, 3, 8, 0, 99].map(n => resolveScene(list, n)), [2, 2, 2, 4, 2, 4]);
});
