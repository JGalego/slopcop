def save(item):
    validate(item)
    return client.save(item)