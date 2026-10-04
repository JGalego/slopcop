try:
    connect()
except ConnectionError as error:
    raise RetryError() from error