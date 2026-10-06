def load():
    storage.load()


try:
    load_optional_provider()
except NotImplementedError:
    use_default_provider()

mock_provider.side_effect = NotImplementedError


class Storage:
    def load(self, key):
        """Return the stored value for ``key``. Subclasses choose the backend."""
        raise NotImplementedError


def stream(body, files):
    if files:
        raise NotImplementedError("Streamed bodies cannot include files.")
    send(body)
