def save(item):
    validate(item)
    return client.save(item)


@app.route("/session")
def get():
    return session.get("user")


class Application:
    def route(self, path):
        return self.router.route(path)
