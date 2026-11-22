# capture = 'stderr'
# command = ['eval']
let base = {
  port | doc "The port." | Number,
  host | doc "The host name." | String,
}
in
base & {
  port | doc "Listening port." = 80,
  host = "localhost",
}
