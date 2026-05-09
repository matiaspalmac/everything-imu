# VQF oracle fixtures

`vqf_oracle.json` is produced by `regenerate.py` and committed to the repo.
CI does NOT run Python — only Rust replays this JSON.

## Status

**Deferred**: `vqf_oracle.json` is **not yet generated**. The `vqf` PyPI package
does not currently build on Python 3.14 (the latest stable). Maintainer needs
Python 3.11 or 3.12 to regenerate.

`crates/imu-fusion/tests/vqf_oracle.rs` is gated behind `#[ignore]` until the
fixture lands.

## Refresh

```bash
# Use Python 3.11 or 3.12; vqf does not build on 3.14
pyenv install 3.12.7
pyenv local 3.12.7
python -m venv .venv
source .venv/bin/activate    # or .venv\Scripts\activate on Windows
pip install vqf numpy
cd crates/imu-fusion/fixtures
python regenerate.py
```

This rewrites `vqf_oracle.json`. Commit the changed file.

## Why Python?

The `vqf` PyPI package provides a reference implementation.
Output is bit-identical to native C++. Acts as an authoritative oracle
for our implementation without requiring C++ build tooling in CI.
