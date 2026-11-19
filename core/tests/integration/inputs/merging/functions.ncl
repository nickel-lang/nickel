# test.type = 'pass'
let u = { a = fun x => x } in
[
  # Merge is idempotent on functions.
  (u & u).a 1 == 1,
  # Functions are merged pointwise.
  ((fun x => { a = x }) & (fun x => { b = x + 1 })) 1 == { a = 1, b = 2 },
  ((fun x y => { a = x }) & (fun x y => { b = y })) 1 2 == { a = 1, b = 2 },
  ((match { 1 => { a = 1 }, _ => {} }) & (fun x => { b = x })) 1 == { a = 1, b = 1 },
  ({ f = fun x => { a = x } } & { f = fun x => { b = x } }).f 3 == { a = 3, b = 3 },
  # The merge of the results is lazy: incompatible functions only fail when applied.
  std.is_function ((fun x => 1) & (fun x => 2)),
]
|> std.test.assert_all
