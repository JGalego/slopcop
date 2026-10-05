def load():
    storage.load()


try:
    load_optional_provider()
except NotImplementedError:
    use_default_provider()

mock_provider.side_effect = NotImplementedError