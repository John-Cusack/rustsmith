# Grading image (Track H: parameterized toolchain, no hardcoded repo).
#
# Python repos (pytest runner; pytest never writes into the tree, so the
# image needs no writable mounts):
#   docker build -f containers/grading.Dockerfile -t <tag> containers/
#
# Compiled/CTest repos (later track):
#   docker build -f containers/grading.Dockerfile --build-arg BASE_IMAGE=gcc:14 -t <tag> containers/
#   (cmake + perf via apt; CTest writes `Testing/` into the build tree, which
#   the sandbox bind-mounts writable per `ImageSpec.writable` at grade time.)
#
# The same selection lives in code (`CompositeAdapter::image()` union over the
# active frontends + spine); the tag derives from the spec, so different
# toolchains never share one. No oracle, no held-out, no store.db baked in.
# Ephemeral + --network=none at runtime.
ARG BASE_IMAGE=python:3.11-slim
FROM ${BASE_IMAGE}
ARG BASE_IMAGE
RUN if [ "${BASE_IMAGE#gcc}" != "${BASE_IMAGE}" ]; then \
        apt-get update && apt-get install -y cmake linux-perf && rm -rf /var/lib/apt/lists/*; \
    else \
        pip install --no-cache-dir "pytest>=7.2" && rm -rf /root/.cache; \
    fi
WORKDIR /artifact
