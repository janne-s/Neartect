import { readSpace, SPACE_KIND } from './space.js';

const floor = (material = 'concrete', z = 0) => ({ kind: 'plane', name: 'Floor', position: [0, 0, z], size: [6, 0, 6], tiltDegrees: 90, material });

export const SITUATIONS = Object.freeze({
  wallside: {
    label: 'By a wall',
    objects: [floor(), { kind: 'plane', name: 'Wall', position: [0, -1, 1.5], size: [6, 0, 3], material: 'brick' }],
    microphone: [0, -0.95, 1.2], arrival: { azimuthDegrees: 30, elevationDegrees: 10 }
  },
  corner: {
    label: 'In a corner',
    objects: [{ kind: 'corner', name: 'Corner', position: [0, 0, 0], size: [3, 3, 2.5], material: 'plaster' }],
    microphone: [0.15, 0.15, 0.15], arrival: { azimuthDegrees: 45, elevationDegrees: 20 }
  },
  niche: {
    label: 'In a niche',
    objects: [floor(), { kind: 'recess', name: 'Niche', position: [0, -1, 1.2], size: [0.6, 0.4, 0.8], material: 'plaster' }],
    microphone: [0, -1.2, 1.2], arrival: { azimuthDegrees: 0, elevationDegrees: 5 }
  },
  grating: {
    label: 'Under a grating',
    objects: [floor('concrete', -0.6), { kind: 'grating', name: 'Grating', position: [0, 0, 0], size: [1.2, 0, 1.2], tiltDegrees: 90, material: 'rigid', pattern: { periodMetres: [0.034, 0.034], barMetres: [0.004, 0.004], thicknessMetres: 0.03 } }],
    microphone: [0, 0, -0.3], arrival: { azimuthDegrees: 0, elevationDegrees: 60 }
  },
  pipe: {
    label: 'At a pipe mouth',
    objects: [floor(), { kind: 'pipe', name: 'Pipe', position: [0, 0, 0.5], size: [0.3, 2, 0.3], yawDegrees: 90, material: 'concrete' }],
    microphone: [1.03, 0, 0.5], arrival: { azimuthDegrees: 90, elevationDegrees: 10 }
  },
  doorway: {
    label: 'In a doorway',
    objects: [floor('woodFloor'), { kind: 'wall', name: 'Wall', position: [0, 0, 1.25], size: [5, 0, 2.5], material: 'plaster', opening: { shape: 'rectangle', sizeMetres: [0.9, 2.1], offsetMetres: [0, -0.2] } }],
    microphone: [0.2, 0.05, 1.5], arrival: { azimuthDegrees: 0, elevationDegrees: 0 }
  },
  water: {
    label: 'Over water',
    objects: [floor('water')],
    microphone: [0, 0, 0.2], arrival: { azimuthDegrees: 0, elevationDegrees: 5 }
  }
});

export function situationSpace(id, current) {
  const { objects, microphone, arrival } = SITUATIONS[id];
  return readSpace({ kind: SPACE_KIND, air: current.air, render: current.render, objects, microphones: [{ position: microphone }], arrival }, objects.length - 1);
}
