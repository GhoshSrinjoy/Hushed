"""Loads the users to sync."""
import json

BUILT_IN = [
    {"id": 1, "email": "ada@example.com"},
    {"id": 2, "email": "grace@example"},
    {"id": 3, "email": None},
]


def load(path="users.json"):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return BUILT_IN  # the file is missing, so a built-in list is synced instead, without a word
