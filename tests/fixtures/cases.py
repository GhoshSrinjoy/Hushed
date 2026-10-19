"""Every way a script can swallow, handle or re-raise an error. Used by tests/test_run.py."""
import contextlib
import json
import logging
import threading

logging.disable(logging.CRITICAL)
log = logging.getLogger("cases")


def fail(kind="value"):
    if kind == "value":
        raise ValueError("bad value")
    raise TypeError("bad type")


def broad_print():
    try:
        fail()
    except Exception as e:
        print(f"error: {e}")


def bare_pass():
    try:
        fail()
    except:  # noqa: E722
        pass


def specific_fallback():
    try:
        return {}["missing"]
    except KeyError:
        return "default"


def logged_with_traceback():
    try:
        fail()
    except ValueError:
        log.exception("failed")


def logged_message_only():
    try:
        fail()
    except ValueError as e:
        log.warning("failed: %s", e)


def reraised_then_caught():
    try:
        try:
            fail()
        except ValueError:
            raise
    except ValueError:
        pass


def second_clause():
    try:
        fail("type")
    except ValueError:
        print("never")
    except TypeError:
        pass


def suppressed():
    with contextlib.suppress(TypeError):
        fail("type")


def finally_return():
    try:
        fail()
    finally:
        return "dropped"  # noqa: B012


def converted():
    try:
        fail()
    except ValueError as e:
        raise RuntimeError("wrapped") from e


def caller_of_converted():
    try:
        converted()
    except RuntimeError:
        pass


def from_library():
    try:
        json.loads("{")
    except json.JSONDecodeError:
        pass


def from_c():
    try:
        int("x")
    except ValueError:
        pass


def in_loop(n=50):
    for _ in range(n):
        try:
            fail()
        except ValueError:
            continue


def thread_dies():
    worker = threading.Thread(target=fail, name="worker")
    worker.start()
    worker.join()


def generator():
    def numbers():
        yield 1
        raise ValueError("in generator")

    try:
        for _ in numbers():
            pass
    except ValueError:
        pass


def control_flow():
    try:
        next(iter([]))
    except StopIteration:
        pass


def never_called():
    pass


if __name__ == "__main__":
    for case in (broad_print, bare_pass, specific_fallback, logged_with_traceback,
                 logged_message_only, reraised_then_caught, second_clause, suppressed,
                 finally_return, caller_of_converted, from_library, from_c, in_loop,
                 thread_dies, generator, control_flow):
        case()
    print("done")
