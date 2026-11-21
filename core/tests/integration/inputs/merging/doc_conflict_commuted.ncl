# test.type = 'error'
# eval = 'full'
#
# [test.metadata]
# error = 'EvalError::MergeIncompatibleDoc'
{ a | doc "bar" } & { a | doc "foo" } & { a = 1 }
