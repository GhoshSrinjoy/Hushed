"""A crash whose real cause was swallowed earlier. Python's traceback shows the KeyError; hushed's
report also shows the error that led to it.

    python examples/crash.py
"""
import hushed  # noqa: F401  (first, so everything after it is watched)

import json


def load_config(path="config.json"):
    try:
        with open(path) as f:
            return json.load(f)
    except Exception:
        return {}  # the file is missing, so the config is silently empty


def connect(config):
    return f"connecting to {config['database_url']}"


print(connect(load_config()))
