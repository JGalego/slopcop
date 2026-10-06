try:
    connect()
except ConnectionError as error:
    raise RetryError() from error

try:
    remove_optional_cache()
except FileNotFoundError:
    pass  # The cache is optional.

try:
    import readline
except ImportError:
    pass

try:
    first = next(records)
except StopIteration:
    pass

try:
    raise LookupError("seed the active exception")
except LookupError:
    pass
