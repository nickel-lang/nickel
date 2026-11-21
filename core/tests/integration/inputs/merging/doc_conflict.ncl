# test.type = 'error'
# eval = 'full'
#
# [test.metadata]
# error = 'EvalError::MergeIncompatibleDoc'
{ a | doc "foo" } & { a | doc "bar" } & { a = 1 }
