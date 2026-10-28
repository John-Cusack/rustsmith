"""Headroom workload: babel number/currency/date formatting + locale lookup.

Doc-driven synthetic, frozen per WORKLOAD.md (rs-port-babel lane):
setup() warms all six locales once (pickle load + merge happen here, so the
probe measures steady-state formatting CPU, not first-load I/O); run()
formats decimals, currencies, percents, dates and datetimes with plain string
locale identifiers (the per-request shape: ``core.Locale.parse`` +
``localedata.__getitem__`` alias resolution + ``numbers.apply`` /
``dates`` pattern application on every call). Returns a checksum over
output lengths. No I/O.
"""

import datetime

LOCALES = ['en_US', 'de_DE', 'fr_FR', 'sv_SE', 'ar_EG', 'ja_JP']
REPS = 300

_D = datetime.date(2007, 4, 1)
_DT = datetime.datetime(2007, 4, 1, 15, 30)


def setup():
    from babel import Locale
    from babel.dates import format_date
    from babel.numbers import format_currency, format_decimal
    for loc in LOCALES:
        Locale.parse(loc)
        format_decimal(1.5, locale=loc)
        format_currency(1.5, 'USD', locale=loc)
        format_date(_D, locale=loc)


def run():
    from babel import Locale
    from babel.dates import format_date, format_datetime
    from babel.numbers import format_currency, format_decimal, format_percent
    total = 0
    for i in range(REPS):
        amount = 1234567.891 + (i % 997) * 0.137
        small = -9876.54321 - (i % 61)
        for loc in LOCALES:
            total += len(format_decimal(amount, locale=loc))
            total += len(format_decimal(small, locale=loc))
            total += len(format_currency(amount, 'USD', locale=loc))
            total += len(format_currency(99.5 + (i % 17), 'EUR', locale=loc))
            total += len(format_percent(small / 100000.0, locale=loc))
            total += len(format_date(_D, format='medium', locale=loc))
            total += len(format_date(_D, format='full', locale=loc))
            total += len(format_datetime(_DT, format='medium', locale=loc))
            total += len(str(Locale.parse(loc)))
    return total
