# The API can replay events after reconnecting.
dedupe(events)

# $ curl http://localhost:3000/users
# $ curl http://localhost:3000/users -H "Accept: application/json"
serve(app)

STATUS = {
    # Server error.
    500: ("internal_server_error", "server_error"),
}

def build_toolbar(toolbar):
    # Main button.
    main_button = Button()
    main_button.flat = True
    toolbar.add(main_button)


def start(controller):
    # Set controller.[[started]] to true.
    controller.started = True

    pull(controller)
