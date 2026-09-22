# Worker run image (Track H: parameterized base, no hardcoded repo).
#
# Python repos (default): docker build -f containers/run.Dockerfile -t rustsmith-run:0.1.0 containers/
# Compiled repos:         docker build -f containers/run.Dockerfile --build-arg BASE_IMAGE=gcc:14 -t <tag> containers/
# (gcc branch keeps the same git/cargo shims + wrapper; the Rust toolchain
# still builds the port, the C/C++ toolchain builds the tree under test.)
ARG BASE_IMAGE=python:3.11-slim
FROM ${BASE_IMAGE}
ARG BASE_IMAGE
RUN if [ "${BASE_IMAGE#gcc}" = "${BASE_IMAGE}" ]; then \
        pip install --no-cache-dir "pytest>=7.2" && rm -rf /root/.cache; \
    fi \
    && mkdir -p /usr/local/rustsmith/bin /run
COPY wrapper.sh /usr/local/rustsmith/bin/wrapper.sh
# Install git/cargo shims first on PATH: /usr/local/rustsmith/shims/{git,cargo} -> wrapper.
RUN mkdir -p /usr/local/rustsmith/shims \
    && printf '#!/bin/sh\nexec /bin/sh /usr/local/rustsmith/bin/wrapper.sh git "$@"\n' > /usr/local/rustsmith/shims/git \
    && printf '#!/bin/sh\nexec /bin/sh /usr/local/rustsmith/bin/wrapper.sh cargo "$@"\n' > /usr/local/rustsmith/shims/cargo \
    && chmod +x /usr/local/rustsmith/shims/git /usr/local/rustsmith/shims/cargo
ENV PATH="/usr/local/rustsmith/shims:${PATH}"
WORKDIR /run
# Confinement is filesystem/process-level (wrapper + worktree perms), never prompt text.
