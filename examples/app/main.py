"""A small app: a few modules and a thread pool. One line at the top watches all of it.

    python examples/app/main.py

The app says every user was synced. The report hushed prints when the app exits says otherwise.
"""
import hushed  # noqa: F401  (first, so everything after it is watched: every module, every thread)

from concurrent.futures import ThreadPoolExecutor

import remote
import users


def main():
    people = users.load()
    with ThreadPoolExecutor(max_workers=4) as pool:
        synced = list(pool.map(remote.push, people))
    print(f"synced {len(synced)} users")


if __name__ == "__main__":
    main()
