try:
    load_profile()
except StorageError as error:
    raise ProfileUnavailable() from error

try:
    load_optional_profile()
except CompatibilityError:
    return None  # Older servers do not expose profiles.