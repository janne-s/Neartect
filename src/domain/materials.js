import { planeReflectionMagnitude } from '../shared/domain/ground.js';

// RoomTreat absorption table (CC BY 4.0); 63 Hz and 8 kHz repeat the nearest band.

export const MATERIAL_ATTRIBUTION = 'Absorption coefficients: RoomTreat, CC BY 4.0 (aggregated from Vorländer, Cox & D\'Antonio, Bies & Hansen, ISO 354 data). Grounds: Delany–Bazley impedance with flow resistivities from Embleton et al.';

const TABLE_BANDS_HZ = [125, 250, 500, 1000, 2000, 4000];

const fromGround = groundType => TABLE_BANDS_HZ.map(hz => 1 - planeReflectionMagnitude(hz, groundType, 1) ** 2);

const AIR_IMPEDANCE = 413;
const reflectionOf = impedance => (impedance - AIR_IMPEDANCE) / (impedance + AIR_IMPEDANCE);
const WATER_REFLECTION = reflectionOf(1.48e6);
const ALUMINIUM_REFLECTION = reflectionOf(2700 * 6320);

export const MATERIALS = Object.freeze({
  rigid: { label: 'Rigid', absorption: [0, 0, 0, 0, 0, 0], reliability: 'A' },
  water: { label: 'Water', absorption: Array(6).fill(1 - WATER_REFLECTION ** 2), reliability: 'A' },
  metal: { label: 'Metal', absorption: Array(6).fill(1 - ALUMINIUM_REFLECTION ** 2), reliability: 'A' },
  concrete: { label: 'Concrete', absorption: [0.01, 0.01, 0.01, 0.02, 0.02, 0.02], reliability: 'B' },
  tile: { label: 'Glazed tile', absorption: [0.01, 0.01, 0.01, 0.01, 0.02, 0.02], reliability: 'B' },
  plaster: { label: 'Plaster', absorption: [0.01, 0.01, 0.02, 0.03, 0.04, 0.05], reliability: 'B' },
  brick: { label: 'Brick', absorption: [0.03, 0.03, 0.03, 0.04, 0.05, 0.07], reliability: 'B' },
  glass: { label: 'Plate glass', absorption: [0.18, 0.06, 0.04, 0.03, 0.02, 0.02], reliability: 'B' },
  gypsum: { label: 'Gypsum board', absorption: [0.29, 0.1, 0.05, 0.04, 0.07, 0.09], reliability: 'B' },
  woodFloor: { label: 'Wood floor', absorption: [0.15, 0.11, 0.1, 0.07, 0.06, 0.07], reliability: 'B' },
  woodPanel: { label: 'Wood panelling', absorption: [0.3, 0.25, 0.2, 0.17, 0.15, 0.1], reliability: 'B' },
  carpet: { label: 'Carpet', absorption: [0.05, 0.15, 0.35, 0.55, 0.65, 0.7], reliability: 'B' },
  curtain: { label: 'Heavy curtain', absorption: [0.14, 0.35, 0.55, 0.72, 0.7, 0.65], reliability: 'B' },
  foam: { label: 'Acoustic foam 50 mm', absorption: [0.08, 0.25, 0.6, 0.9, 0.95, 0.98], reliability: 'B' },
  mineralWool: { label: 'Mineral wool 50 mm', absorption: [0.15, 0.55, 0.9, 1, 1, 1], reliability: 'B' },
  stonyGround: { label: 'Stony ground', absorption: fromGround('stonyGround'), reliability: 'B' },
  soil: { label: 'Soil', absorption: fromGround('soil'), reliability: 'B' },
  grass: { label: 'Grass', absorption: fromGround('grass'), reliability: 'B' },
  forestFloor: { label: 'Forest floor', absorption: fromGround('forestFloor'), reliability: 'B' },
  snow: { label: 'Snow', absorption: fromGround('snow'), reliability: 'B' }
});

export const DEFAULT_MATERIAL = 'concrete';
