"""Headroom workload: PyYAML pure-Python safe load.

Doc-driven synthetic: `yaml.safe_load()` (the pure-Python `SafeLoader`
path, not `CSafeLoader`) over a generated 4000-service document — the
scanner/reader/parser pipeline the 3.8x scan headroom was proven on.
"""

DOC = ""


def setup():
    global DOC
    lines = []
    for i in range(4000):
        lines.append(
            f"  - name: service-{i}\n"
            f"    port: {8000 + i}\n"
            f"    tags: [web, v{i % 7}, prod]\n"
            "    config: {retries: 3, timeout: 30, enabled: true}\n"
        )
    DOC = "services:\n" + "".join(lines)


def run():
    import yaml
    for _ in range(3):
        yaml.safe_load(DOC)
