def publish(event):
    broker.send(event)


def on_shutdown():
    pass  # Framework lifecycle hook.


@app.route("/health")
def health():
    pass
