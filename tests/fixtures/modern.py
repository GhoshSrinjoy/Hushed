"""Things the old instrumented copies broke: __future__ imports, pickling classes defined
in the script, the __main__ guard, script arguments, and frames freed on time."""
from __future__ import annotations

import json
import pickle
import sys
import weakref
from dataclasses import dataclass


@dataclass
class Point:
    x: int
    y: int


class Resource:
    pass


refs = []


def fails():
    resource = Resource()           # freed together with this frame
    refs.append(weakref.ref(resource))
    raise ValueError("bad")


if __name__ == "__main__":
    try:
        fails()
    except ValueError:
        pass
    print(json.dumps({
        "args": sys.argv[1:],
        "pickled": pickle.loads(pickle.dumps(Point(1, 2))) == Point(1, 2),
        "frame_freed": refs[0]() is None,
    }))
    sys.exit(4)
