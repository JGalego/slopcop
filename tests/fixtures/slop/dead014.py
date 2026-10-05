try:
    publish(event)
except NetworkError:
    raise