"""Sends users to a (pretend) remote service. Runs in the app's worker threads."""


def check(user):
    domain = user["email"].split("@")[-1]
    if "." not in domain:
        raise ValueError(f"invalid email for user {user['id']}: {user['email']}")


def push(user):
    try:
        check(user)
        # ... send the user to the service here ...
    except Exception:
        pass  # a failed push looks exactly like a successful one
    return user["id"]
