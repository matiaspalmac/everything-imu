"""Regenerate vqf_oracle.json — deterministic synthetic IMU sequence + reference outputs.

One-time use. Output is the canonical oracle for Rust replay tests.

Usage (offline, by maintainer with Python 3.11 or 3.12 — vqf does NOT yet build
on Python 3.14):

    python -m venv .venv
    source .venv/bin/activate    # or .venv\Scripts\activate on Windows
    pip install vqf numpy
    python regenerate.py

Commit the generated `vqf_oracle.json`. CI never runs Python.
"""
import json
import math
import numpy as np
from vqf import VQF

FS = 200.0          # sample rate (Hz)
TS = 1.0 / FS
N = 200             # frames

np.random.seed(42)

t = np.arange(N) * TS
gyr = np.zeros((N, 3))
acc = np.zeros((N, 3))
mag = np.zeros((N, 3))

# 100 frames static face-up, then 100 frames slow rotation about Z.
for i in range(N):
    if i < 100:
        gyr[i] = [0.0, 0.0, 0.0]
        acc[i] = [0.0, 0.0, 9.80665]
        mag[i] = [1.0, 0.0, 0.5]
    else:
        gyr[i] = [0.0, 0.0, math.sin((i - 100) * 0.05) * 0.5]
        acc[i] = [0.0, 0.0, 9.80665]
        mag[i] = [math.cos((i - 100) * 0.05), math.sin((i - 100) * 0.05), 0.5]

vqf = VQF(TS)
quat6d = np.zeros((N, 4))
quat9d = np.zeros((N, 4))
bias = np.zeros((N, 3))
rest = np.zeros(N, dtype=bool)
mag_dist = np.zeros(N, dtype=bool)

for i in range(N):
    vqf.update(gyr[i], acc[i], mag[i])
    quat6d[i] = vqf.getQuat6D()
    quat9d[i] = vqf.getQuat9D()
    bias[i] = vqf.getBiasEstimate()[0]
    rest[i] = vqf.getRestDetected()
    mag_dist[i] = vqf.getMagDistDetected()

out = {
    "fs": FS,
    "n": N,
    "gyr": gyr.tolist(),
    "acc": acc.tolist(),
    "mag": mag.tolist(),
    "expected": {
        "quat6d": quat6d.tolist(),
        "quat9d": quat9d.tolist(),
        "bias": bias.tolist(),
        "rest_detected": rest.tolist(),
        "mag_dist_detected": mag_dist.tolist(),
    },
}

with open("vqf_oracle.json", "w") as f:
    json.dump(out, f, indent=2)
print(f"wrote vqf_oracle.json with {N} frames at {FS} Hz")
