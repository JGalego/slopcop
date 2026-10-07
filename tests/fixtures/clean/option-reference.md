# Directory options

Each source shares these fields:

* Host **(required)**
  * The address where the directory server can be reached.
  * Example: directory.example.com

* Port **(required)**
  * The port to use when connecting to the server.
  * Example: 636

* Admin filter (optional)
  * A filter that selects which accounts become administrators.
  * Example: (objectClass=adminAccount)

* Mail attribute **(required)**
  * The attribute that holds the account's email address.
  * Example: mail
