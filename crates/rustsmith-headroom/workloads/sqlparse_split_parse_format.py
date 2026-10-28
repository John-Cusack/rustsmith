"""Headroom workload: sqlparse split + parse + format (representative DML/DDL).

Doc-driven synthetic: a 121-statement mixed script (1 CREATE TABLE, 60
INSERTs with value lists, 40 JOINed SELECTs with WHERE/ORDER/LIMIT, 20
commented UPDATEs, ~13 KB) exercised through the three public entry
points the port thesis names — ``split`` (statement splitter),
``parse`` (lexer + grouping engine), ``format`` reindent (full filter
stack) and ``format`` light (keyword_case + strip_comments). One whole
SQL string crosses per call; setup builds the payload once, run()
executes the four paths.
"""

SCRIPT = ""


def setup():
    global SCRIPT
    stmts = [
        "CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER "
        "NOT NULL, total NUMERIC(10,2), placed_at TIMESTAMP DEFAULT now(), "
        "status VARCHAR(20));"
    ]
    for i in range(60):
        stmts.append(
            f"INSERT INTO orders (id, customer_id, total, status) VALUES "
            f"({i}, {i % 17}, {i * 1.5:.2f}, 'shipped');"
        )
    for i in range(40):
        stmts.append(
            "SELECT o.id, c.name, o.total FROM orders o "
            "JOIN customers c ON c.id = o.customer_id "
            f"WHERE o.status = 'shipped' AND o.total > {i} "
            "ORDER BY o.placed_at DESC LIMIT 50;"
        )
    for i in range(20):
        stmts.append(
            f"UPDATE orders SET status = 'refunded' WHERE id = {i}; "
            f"-- refund batch {i}\n"
        )
    SCRIPT = "\n".join(stmts)


def run():
    import sqlparse
    sqlparse.split(SCRIPT)
    sqlparse.parse(SCRIPT)
    sqlparse.format(SCRIPT, reindent=True, keyword_case="upper")
    sqlparse.format(SCRIPT, keyword_case="upper", strip_comments=True)
