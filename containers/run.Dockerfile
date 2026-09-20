FROM python:3.11-slim
RUN pip install --no-cache-dir "pytest>=7.2" && rm -rf /root/.cache
# M1 will add the PATH wrapper, cgroup mounts, worktree tooling here.
WORKDIR /run
