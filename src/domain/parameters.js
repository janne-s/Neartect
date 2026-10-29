import { RANGES } from './space.js';

const row = ([minimum, maximum], unit, label, reliability = null, integer = false) => ({ minimum, maximum, unit, label, reliability, integer });

export const PARAMETERS = Object.freeze({
  positionMetres: row(RANGES.positionMetres, 'm', 'Position'),
  sizeMetres: row(RANGES.sizeMetres, 'm', 'Size'),
  yawDegrees: row(RANGES.yawDegrees, '°', 'Facing'),
  tiltDegrees: row(RANGES.tiltDegrees, '°', 'Tilt'),
  strength: row(RANGES.strength, '', 'Strength', 'C'),
  periodMetres: row(RANGES.periodMetres, 'm', 'Period'),
  barMetres: row(RANGES.periodMetres, 'm', 'Bar'),
  thicknessMetres: row(RANGES.thicknessMetres, 'm', 'Thickness', 'B'),
  temperatureCelsius: row(RANGES.temperatureCelsius, '°C', 'Temperature', 'A'),
  lengthMilliseconds: row(RANGES.lengthMilliseconds, 'ms', 'Filter length'),
  order: row(RANGES.order, '', 'Reflection order', null, true),
  reliefDetail: row(RANGES.reliefDetail, '', 'Relief detail'),
  elementLimit: row(RANGES.elementLimit, '', 'Element limit', null, true),
  movingDetail: row(RANGES.movingDetail, '', 'Detail while moving')
});

export const DISPLAY = Object.freeze({
  periodMetres: { scale: 1000, unit: 'mm' },
  barMetres: { scale: 1000, unit: 'mm' },
  thicknessMetres: { scale: 1000, unit: 'mm' }
});
