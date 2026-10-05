try:
    publish(event)
except NetworkError as error:
    print(error)