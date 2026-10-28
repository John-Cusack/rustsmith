"""Headroom workload: marshmallow nested schema dump/load batch.

Doc-driven synthetic: nested ``User`` schema (nested ``Address``,
list of nested ``Order`` line items, validated str/int/float/bool/
email/datetime fields) over 500 synthetic objects; run() executes
dump(many=True) + load(many=True) x4 — the per-request validation
hot path the lane thesis names (rerank §3 lane 3). Setup builds the
objects once; run() executes the kernel ops. No I/O.
"""

SCHEMA = None
ORDERS = None
USERS = None
DUMPED = None


def setup():
    global SCHEMA, USERS, DUMPED
    from marshmallow import Schema, fields

    class AddressSchema(Schema):
        street = fields.Str(required=True)
        city = fields.Str(required=True)
        zip_code = fields.Str(required=True, validate=lambda s: len(s) == 5)
        country = fields.Str(load_default="US")

    class OrderSchema(Schema):
        item = fields.Str(required=True)
        quantity = fields.Int(required=True, validate=lambda n: 0 < n < 1000)
        price = fields.Float(required=True)
        gift_wrap = fields.Bool(load_default=False)

    class UserSchema(Schema):
        name = fields.Str(required=True)
        email = fields.Email(required=True)
        age = fields.Int(validate=lambda n: 0 <= n <= 150)
        address = fields.Nested(AddressSchema, required=True)
        orders = fields.List(fields.Nested(OrderSchema), required=True)

    SCHEMA = UserSchema()
    USERS = [
        {
            "name": f"user-{i}",
            "email": f"user-{i}@example.com",
            "age": 20 + (i % 60),
            "address": {
                "street": f"{i} Main St",
                "city": "Springfield",
                "zip_code": f"{10000 + (i % 89999):05d}",
            },
            "orders": [
                {
                    "item": f"widget-{j}",
                    "quantity": 1 + ((i + j) % 9),
                    "price": 1.5 + (i % 100),
                }
                for j in range(3)
            ],
        }
        for i in range(500)
    ]
    DUMPED = SCHEMA.dump(USERS, many=True)


def run():
    for _ in range(4):
        dumped = SCHEMA.dump(USERS, many=True)
        loaded = SCHEMA.load(DUMPED, many=True)
        assert len(dumped) == 500 and len(loaded) == 500
