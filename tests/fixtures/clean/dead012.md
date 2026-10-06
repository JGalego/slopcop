## Configuration

Set `timeout_ms` to the request deadline. The default is 5000.

# QUICK LINKS
# -----------

Read the configuration reference before deployment.

<details>

	```yaml
	# This is configuration, not a heading.
	timeout_ms: 5000
	```

</details>

### client.get(url)
### client.post(url)

Sends a request with the named method and returns the response.

    # Indented code is not a heading.
    client.get("/health")
