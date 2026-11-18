# test.type = 'pass'
let Assert = std.test.Assert in

[
  # Only one side has a documentation, in either order.
  ({ a | doc "foo" } & { a = 1 }) == { a = 1 },
  ({ a = 1 } & { a | doc "foo" }) == { a = 1 },

  # Both sides have the very same documentation.
  ({ a | doc "foo" = 1 } & { a | doc "foo" }) == { a = 1 },
  ({ a | doc "foo" } & { a | doc "foo" } & { a = 1 }) == { a = 1 },

  # Documentation of different fields doesn't interact.
  ({ a | doc "foo" = 1 } & { b | doc "bar" = 2 }) == { a = 1, b = 2 },

  # A value doesn't conflict with a contract that has its own documentation.
  let Contract = { a | doc "from the contract" | Number } in
  ({ a | doc "from the value" = 1 } | Contract) == { a = 1 },

  # Merging a record with itself is fine.
  let r = { a | doc "foo" = 1 } in
  (r & r) == { a = 1 },
]
|> std.test.assert_all
