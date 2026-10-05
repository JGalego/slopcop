try:
    publish(event)
except NetworkError as error:
    raise PublishError("event delivery failed") from error