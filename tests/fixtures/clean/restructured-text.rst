Quickstart
==========

.. module:: example

Install the package with :command:`pip` and import :class:`~example.Client`.
The client reads :data:`example.DEFAULTS` and :envvar:`EXAMPLE_TOKEN` on startup.
Use :meth:`~example.Client.get` for reads and :meth:`~example.Client.post` for writes.
Errors raise :exc:`~example.RequestError` with the failed :class:`~example.Response`.
Retries follow :data:`example.RETRY_POLICY` unless :attr:`Client.retries` is set.
Timeouts come from :attr:`Client.timeout` when :func:`~example.request` omits one.
See :doc:`configuration` and :ref:`authentication` for the remaining settings.
Streaming responses are covered in :doc:`streaming` with complete examples.

.. versionadded:: 2.0
    The client accepts a timeout in seconds.

.. code-block:: python

    # Create a client for the default endpoint.
    client = Client()

Responses decode JSON automatically::

    # Read the decoded body
    data = client.get("/items").json()

>>> client.get("/health").status_code
200

:param timeout: Seconds to wait for the server.
:returns: The decoded response body.
