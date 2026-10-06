try:
    publish(event)
except NetworkError as error:
    raise PublishError("event delivery failed") from error


async def save(progress):
    try:
        await store.save(progress)
    except (ConflictError, NotFoundError):
        raise
    except Exception as error:
        mark_storage_failed(error)
