# test.type = 'error'
# eval = 'full'
#
# [test.metadata]
# error = 'EvalError::MergeIncompatibleDoc'
let Contract = { a | doc "from the contract" | Number } in
# Contract application behaves like live merging: two different `doc`
# annotations on the same field are an error there as well.
{ a | doc "from the value" = 1 } | Contract
