"""A small data job. It prints "done", but part of the work quietly failed.

    hushed run examples/pipeline.py
"""
import json

ROWS = [
    '{"id": 1, "price": "9.99"}',
    '{"id": 2, "price": "N/A"}',
    "{broken json",
    '{"id": 4, "price": "4.50"}',
]


def load_settings(path="settings.json"):
    try:
        with open(path) as f:
            return json.load(f)
    except Exception:
        return {"currency": "USD"}  # the file is missing, so defaults are used without a word


def parse(row):
    try:
        return json.loads(row)
    except Exception:
        return None  # bad rows simply vanish


def price(record):
    try:
        return float(record["price"])
    except Exception as e:
        print(f"warning: {e}")  # a message, but no traceback and no row id
        return 0.0


def main():
    settings = load_settings()
    records = [r for r in map(parse, ROWS) if r]
    total = sum(price(r) for r in records)
    print(f"done: {len(records)} records, total {total:.2f} {settings['currency']}")


if __name__ == "__main__":
    main()
