FROM python:3.11-slim
RUN pip install --no-cache-dir "pytest>=7.2" && rm -rf /root/.cache \
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
