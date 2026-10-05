try:
    publish(event)
except NetworkError:
    retry(event)