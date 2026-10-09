async def start(connection, listener, session):
    # The proxy accepts traffic before the backend does, so block until the backend answers.
    await connection.wait_until_ready()

    # Late listeners miss the first burst of events; attach before the session is announced.
    session.attach_listener_to_session_events(listener)

    # Three attempts matches the load balancer's own timeout budget.
    policy = RetryPolicy.build_default_retry_policy_for_client(session)

    return policy
