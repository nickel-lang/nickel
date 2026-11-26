# test.type = 'error'
#
# [test.metadata]
# error = 'ParseError::DuplicateIdentInRecordPattern'
#
# [test.metadata.expectation]
# ident = 'x'
let { a = x @ { c }, b = x } = { a = { c = 1 }, b = 2 } in
x
