# test.type = 'error'
# eval = 'full'
#
# [test.metadata]
# error = 'EvalError::MergeIncompatibleDoc'
{ a.b | doc "foo" = 1 } & { a.b | doc "bar" = 1 }
