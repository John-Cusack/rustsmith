"""Equivalence: dump canonical rows for the imported email_validator build.

Emits one JSON doc: {"rows": [...]} where each row is
{"addr", "opts", "result"} with result = ["ok", normalized, ascii_email,
smtputf8, local_part, domain, ascii_domain, display_name] or
["err", exc_type, message]. Run under stock, then under the tier-1
patched build (apply tier1.patch to a temp copy, prepend to PYTHONPATH),
and diff the row lists (order is deterministic).
Usage: <venv-python> equiv_check.py [out.jsonl]
"""

import itertools
import json
import sys

ADDRS = [
    "user.name+tag-1@mail1.example.com",
    "USER@EXAMPLE.COM",
    "Mixed.Case@Mail.Example.COM",
    '"quoted name 1"@example.com',
    '"a@b"@example.com',
    "Üser-1@münchen.de",
    "user@日本語.jp",
    "user@xn--mnchen-3ya.de",
    "user@xn--foobar.com",
    "user@XN--MNCHEN-3YA.DE",
    "bad-1-no-at-sign.example.com",
    "user1@bad_domain_1.com",
    "user leading@example.com",
    ".leading@example.com",
    "trailing.@example.com",
    "double..dot@example.com",
    "user@localhost",
    "user@gmail.test",
    "user@" + "l" * 64 + ".com",
    "x" * 70 + "@long.example.com",
    "user@[127.0.0.1]",
    "user@[IPv6:::1]",
    "user@[bad-literal]",
    "a@b.co",
    "test@test",
    "Abc@iana.org",
    "first.last@iana.org",
    "disposable.style.email.with+symbol@example.com",
    "much.more.unusual@example.com",
    "admin@mailserver1",
    "!#$%&'*+-/=?^_`{}|~@example.org",
    "user@exam_ple.com",
    "user@-leading.com",
    "user@trailing-.com",
    "user@a--b.com",
    "user@ab--cd.com",
    "user@xn--a.com",
    "üñîçødé@example.com",
    "user@münster-1.de",
    "a@a." + "b" * 63 + ".com",
    "Display Name <user@example.com>",
    "<user@example.com>",
    "use\u0308r@example.com",
    '"a\u0301b"@example.com',
    "<\u0338user@example.com>",
    "user@exam\u0327ple.com",
    "é@example.com",
    "e\u0301@example.com",
    '" "@example.com',
    "user@exam ple.com",
    "user@例え.jp",
    "USER@MÜNCHEN.DE",
]

OPTSETS = [
    {},
    {"allow_smtputf8": False},
    {"strict": True},
    {"test_environment": True},
    {"globally_deliverable": False},
    {"allow_quoted_local": True},
    {"allow_domain_literal": True},
    {"allow_display_name": True},
    {"allow_empty_local": True},
]


def probe(addr, opts):
    from email_validator import EmailNotValidError, validate_email
    try:
        r = validate_email(addr, check_deliverability=False, **opts)
        return ["ok", r.normalized, r.ascii_email, r.smtputf8, r.local_part,
                r.domain, r.ascii_domain, str(r.display_name)]
    except EmailNotValidError as e:
        return ["err", type(e).__name__, str(e)]
    except Exception as e:  # noqa: BLE001 -- must surface, never mask
        return ["OTHER", type(e).__name__, str(e)]


def main():
    import email_validator
    rows = [{"addr": a, "opts": o, "result": probe(a, o)}
            for a, o in itertools.product(ADDRS, OPTSETS)]
    doc = {"package": email_validator.__file__, "rows": rows,
           "mismatches_vs": None, "mismatch_count": 0}
    if len(sys.argv) > 1:
        with open(sys.argv[1], "w") as f:
            f.write(json.dumps(doc, sort_keys=True, ensure_ascii=False) + "\n")
    else:
        print(json.dumps(doc, sort_keys=True, ensure_ascii=False))


if __name__ == "__main__":
    main()
