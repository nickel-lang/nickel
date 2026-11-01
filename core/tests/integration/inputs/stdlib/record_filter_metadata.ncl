# test.type = 'pass'
let keep_all_but_zone = fun field _value => field != "zone" in
[
  # priorities are preserved
  (
    (std.record.filter keep_all_but_zone { zone = "A", records | default = [] })
    & { records = ["a"] }
  ) == { records = ["a"] },
  # not_exported is preserved
  (
    std.record.filter keep_all_but_zone { zone = "A", a = 1, b | not_exported = 2 }
    |> std.serialize 'Json
  ) == std.serialize 'Json { a = 1 },
  # the record is frozen, as with std.record.remove
  (
    (std.record.filter keep_all_but_zone { foo = bar + 1, bar = 0, zone = "A" })
    & { bar | force = 1 }
  ) == { foo = 1, bar = 1 },
  # contracts don't propagate through later merges, since the record is frozen
  (
    (std.record.filter keep_all_but_zone { zone = "A", records | Array String = [] })
    & { records | force = [1] }
  ) == { records = [1] },
  # filtering still drops the rejected fields
  std.record.filter (fun _name x => x % 2 == 0) { even = 2, odd = 3 } == { even = 2 },
  std.record.filter (fun _name _x => false) { a = 1, b = 2 } == {},
  std.record.filter (fun _name _x => true) {} == {},
  # empty optional fields are dropped, as they can't satisfy the predicate
  std.record.fields_with_opts (std.record.filter (fun _name _x => true) { a = 1, b | optional }) == ["a"],
  std.record.fields_with_opts (std.record.filter (fun _name _x => false) { a | optional = 1 }) == [],
]
|> std.test.assert_all
