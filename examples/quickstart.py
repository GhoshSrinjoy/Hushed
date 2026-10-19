"""Quick start: `import hushed` as the first line, then run the program the usual way.

    python examples/quickstart.py
"""
import hushed  # noqa: F401  (first, so everything after it is watched)

import json


def load_config(path="config.json"):
    try:
        with open(path) as f:
            return json.load(f)
    except Exception:
        return {}  # no config file? carry on with defaults, without a word


def retries(config):
    try:
        return int(config["retries"])
    except KeyError:
        return 3  # a specific, expected error: hushed counts this one as caught on purpose


config = load_config()
print(f"job finished with {retries(config)} retries")
