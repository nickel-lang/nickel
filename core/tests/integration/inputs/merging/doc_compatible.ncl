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

  # A value with the same documentation as the contract is fine.
  let Contract = { a | doc "same doc" | Number } in
  ({ a | doc "same doc" = 1 } | Contract) == { a = 1 },

  # Merging a record with itself is fine.
  let r = { a | doc "foo" = 1 } in
  (r & r) == { a = 1 },

  # A field defined twice in the same literal with equal documentations is fine.
  { a | doc "foo" = 1, a | doc "foo" = 1 } == { a = 1 },
]
|> std.test.assert_all
