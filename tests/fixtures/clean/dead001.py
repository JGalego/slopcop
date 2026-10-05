try:
    connect()
except ConnectionError as error:
    raise RetryError() from error

try:
    remove_optional_cache()
except FileNotFoundError:
    pass  # The cache is optional.