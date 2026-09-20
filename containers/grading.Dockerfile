FROM python:3.11-slim
# Pinned era toolchain: Python 3.11, pytest>=7.2 (fixture era).
RUN pip install --no-cache-dir "pytest>=7.2" && rm -rf /root/.cache
WORKDIR /artifact
# No oracle, no held-out, no store.db baked in. Ephemeral + --network=none at runtime.
