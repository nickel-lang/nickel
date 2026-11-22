# capture = 'stderr'
# command = ['eval']

# Long documentation is cut in the error message, on a character boundary.
{
  port
    | doc "Päivä ja yö: the port the server listens on. Clients connect here, so it must be reachable from outside the machine."
    = 80,
}
& {
  port | doc "Listening port." | Number,
}
