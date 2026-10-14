"""Headroom workload: python-multipart event-heavy form parse.

Doc-driven synthetic in the shape of lane rs-upstream-sweep-5 (~1.27x on
this form): many small fields plus small files, body fed in 4KB chunks so
the per-write Python dispatch (`_internal_write`/`data_callback`)
dominates instead of one-shot C `memmem` scanning.
"""

FEED = []
HEADERS = {}


def setup():
    global FEED, HEADERS
    boundary = "BoUnDaRy12345"
    chunks = []
    for i in range(200):
        chunks.append(
            b"--" + boundary.encode() + b"\r\n"
            b'Content-Disposition: form-data; name="f%d"\r\n\r\n' % i
            + b"value-%d-xxxxxxxxxxxxxxxx\r\n" % i
        )
    for _ in range(20):
        chunks.append(
            b"--" + boundary.encode() + b"\r\n"
            b'Content-Disposition: form-data; name="file"; filename="f.bin"\r\n'
            b"Content-Type: application/octet-stream\r\n\r\n"
            + b"B" * 20000 + b"\r\n"
        )
    body = b"".join(chunks) + b"--" + boundary.encode() + b"--\r\n"
    HEADERS = {
        "Content-Type": f"multipart/form-data; boundary={boundary}",
        "Content-Length": str(len(body)),
    }
    FEED = [body[i:i + 4096] for i in range(0, len(body), 4096)]


def on_file(f):
    f.file_object.seek(0)
    while f.file_object.read(65536):
        pass


def on_field(f):
    pass


def run():
    from multipart import create_form_parser
    for _ in range(10):
        p = create_form_parser(HEADERS, on_field, on_file)
        for c in FEED:
            p.write(c)
