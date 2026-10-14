"""Headroom workload: pygments Python highlighting.

Doc-driven synthetic: `highlight()` a ~120KB synthetic Python module with
the stock `PythonLexer`/`HtmlFormatter` (the documented entry points).
Source is generated, so the workload is byte-identical on every box.
"""

SRC = ""


def setup():
    global SRC
    funcs = []
    for i in range(600):
        funcs.append(
            f"def handler_{i}(request, timeout=30, retries=3):\n"
            f"    '''Process item {i} with a docstring.'''\n"
            f"    try:\n"
            f"        value = request.get('key_{i}', default=None)\n"
            f"        items = [x * 2 for x in range(100) if x % {i % 7 + 1} == 0]\n"
            f"        return {{'ok': True, 'n': len(items), 'v': value!r}}\n"
            f"    except (KeyError, ValueError) as exc:\n"
            f"        raise RuntimeError(f'handler {i} failed: {{exc}}') from exc\n"
        )
    SRC = "\n".join(funcs)


def run():
    from pygments import highlight
    from pygments.formatters import HtmlFormatter
    from pygments.lexers import PythonLexer
    for _ in range(3):
        highlight(SRC, PythonLexer(), HtmlFormatter())
