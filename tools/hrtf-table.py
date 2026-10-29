# Converts a SADIE II SOFA file (SimpleFreeFieldHRIR) into the measured head
# table the core embeds: each direction as a minimum-phase response plus a
# pure delay, both ears folded into the left by symmetry, the responses
# expanded in a few principal spectra, and the weights resampled onto rings
# of fixed elevation at even azimuth steps.
#
#   python3 tools/hrtf-table.py D1_48K_24bit_256tap_FIR_SOFA.sofa core/data/ku100.bin
#
# Needs h5py and numpy (tool only; the app does not).

import struct
import sys

import h5py
import numpy as np

COMPONENTS = 16
TAPS = 256
AZIMUTH_STEP_DEGREES = 5.0
FFT_SIZE = 1024
DELAY_BAND_HZ = (300.0, 1500.0)


def minimum_phase(h):
    log_magnitude = np.log(np.maximum(np.abs(np.fft.fft(h, FFT_SIZE)), 1e-8))
    cepstrum = np.fft.ifft(log_magnitude).real
    fold = np.zeros(FFT_SIZE)
    fold[0] = 1
    fold[1:FFT_SIZE // 2] = 2
    fold[FFT_SIZE // 2] = 1
    return np.exp(np.fft.fft(cepstrum * fold))


def excess_delay(h, frequencies, band):
    phase = np.unwrap(np.angle(np.fft.fft(h, FFT_SIZE) / minimum_phase(h)))
    return -np.polyfit(2 * np.pi * frequencies[band], phase[band], 1)[0]


def ring_resample(azimuths, values, grid):
    order = np.argsort(azimuths)
    a = azimuths[order]
    v = values[order]
    a = np.concatenate([a[-1:] - 360, a, a[:1] + 360])
    v = np.concatenate([v[-1:], v, v[:1]])
    return np.stack([np.interp(grid, a, v[:, k]) for k in range(v.shape[1])], axis=1)


def main(source, target, check=None):
    sofa = h5py.File(source, 'r')
    rate = float(sofa['Data.SamplingRate'][0])
    ir = sofa['Data.IR'][:]
    position = sofa['SourcePosition'][:]
    azimuth = position[:, 0] % 360
    elevation = position[:, 1]
    frequencies = np.fft.fftfreq(FFT_SIZE, 1 / rate)
    band = (frequencies > DELAY_BAND_HZ[0]) & (frequencies < DELAY_BAND_HZ[1])
    bins = FFT_SIZE // 2 + 1

    index = {(round(a, 3), round(e, 3)): i for i, (a, e) in enumerate(zip(azimuth, elevation))}
    mirror = np.array([index.get((round((-a) % 360, 3), round(e, 3)), -1) for a, e in zip(azimuth, elevation)])

    spectra = np.zeros((len(position), bins), complex)
    delays = np.zeros(len(position))
    for i in range(len(position)):
        left = minimum_phase(ir[i, 0])[:bins]
        left_delay = excess_delay(ir[i, 0], frequencies, band)
        j = mirror[i]
        if j >= 0:
            spectra[i] = (left + minimum_phase(ir[j, 1])[:bins]) / 2
            delays[i] = (left_delay + excess_delay(ir[j, 1], frequencies, band)) / 2
        else:
            spectra[i] = left
            delays[i] = left_delay
    delays -= delays[index[(0.0, 0.0)]]

    rings = np.unique(elevation)
    edges = np.concatenate([[-90], (rings[1:] + rings[:-1]) / 2, [90]])
    solid = {e: np.sin(np.radians(edges[k + 1])) - np.sin(np.radians(edges[k])) for k, e in enumerate(rings)}
    weight = np.array([solid[e] / (elevation == e).sum() for e in elevation])

    stacked = np.concatenate([spectra.real, spectra.imag], axis=1)
    mean = (weight[:, None] * stacked).sum(0) / weight.sum()
    centred = (stacked - mean) * np.sqrt(weight)[:, None]
    u, s, vt = np.linalg.svd(centred, full_matrices=False)
    coefficients = np.concatenate([np.ones((len(position), 1)), u[:, :COMPONENTS] * s[:COMPONENTS] / np.sqrt(weight)[:, None]], axis=1)
    basis_stacked = np.concatenate([mean[None, :], vt[:COMPONENTS]], axis=0)
    basis_spectra = basis_stacked[:, :bins] + 1j * basis_stacked[:, bins:]
    basis = np.fft.irfft(basis_spectra, FFT_SIZE, axis=1)[:, :TAPS]

    residual = (s[COMPONENTS:] ** 2).sum() / (weight[:, None] * stacked ** 2).sum()
    tail = (np.fft.irfft(basis_spectra, FFT_SIZE, axis=1)[:, TAPS:] ** 2).sum() / (np.fft.irfft(basis_spectra, FFT_SIZE, axis=1) ** 2).sum()
    print(f'residual energy {10 * np.log10(residual):.1f} dB, truncated tail {10 * np.log10(max(tail, 1e-30)):.1f} dB')

    grid = np.arange(0, 360, AZIMUTH_STEP_DEGREES)
    ring_values = []
    for e in rings:
        members = elevation == e
        values = np.concatenate([coefficients[members], delays[members, None]], axis=1)
        if members.sum() == 1:
            ring_values.append(np.repeat(values, len(grid), axis=0))
        else:
            ring_values.append(ring_resample(azimuth[members], values, grid))
    ring_values = np.array(ring_values)

    sets = COMPONENTS + 1
    with open(target, 'wb') as out:
        out.write(struct.pack('<4I', sets, TAPS, len(rings), len(grid)))
        out.write(struct.pack('<f', rate))
        out.write(np.asarray(rings, '<f4').tobytes())
        out.write(np.asarray(ring_values[:, :, sets], '<f4').tobytes())
        out.write(np.asarray(ring_values[:, :, :sets], '<f4').tobytes())
        out.write(np.asarray(basis, '<f4').tobytes())
    if check:
        np.savez(check, spectra=spectra, delays=delays, azimuth=azimuth, elevation=elevation, weight=weight)


if __name__ == '__main__':
    main(*sys.argv[1:4])
