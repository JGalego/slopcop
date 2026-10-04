try:
    load_profile()
except StorageError as error:
    raise ProfileUnavailable() from error