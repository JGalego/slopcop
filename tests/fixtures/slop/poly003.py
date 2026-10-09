async def start(connection, listener, session):
    # Wait until the connection is ready.
    await connection.wait_until_ready()

    # Attach the listener to the session.
    session.attach_listener_to_session_events(listener)

    # Build the retry policy for the client.
    policy = RetryPolicy.build_default_retry_policy_for_client(session)

    return policy
