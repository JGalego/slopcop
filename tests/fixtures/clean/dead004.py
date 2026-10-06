try:
    load_profile()
except StorageError as error:
    raise ProfileUnavailable() from error

try:
    load_optional_profile()
except CompatibilityError:
    return None  # Older servers do not expose profiles.


def is_ipv4(address):
    try:
        socket.inet_aton(address)
    except OSError:
        return False
    return True
