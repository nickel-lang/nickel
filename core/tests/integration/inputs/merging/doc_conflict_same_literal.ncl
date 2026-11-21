# test.type = 'error'
# eval = 'full'
#
# [test.metadata]
# error = 'EvalError::MergeIncompatibleDoc'
{ a | doc "foo" = 1, a | doc "bar" = 2 }
