---
feature: Field coercions
start-date: 2026-11-02
author: Mathieu Boespflug
---

# Field coercions

## Motivation

### Sets and sequences represented as arrays

Consider the following snippet drawn from my NixOS configuration:

```nix
  users.mboes = {
    isNormalUser = true;
    uid = 1000;
    extraGroups = [
      "audio"
      "docker"
      # ...
    ];
  };
```

The value of the `extraGroups` field is an array, aka a finite
(ordered) sequence of elements, but it represents a set, since the
order of elements is irrelevant.

Consider also a snippet from a workflow in this repository:

```yaml
jobs:
  build-and-test:
    steps:
    - name: Checking out the repository
      uses: actions/checkout@v7
    - name: Install Nix
      uses: cachix/install-nix-action@v31
```

The value of `jobs.build-and-test.steps` is also an array. It
represents a sequence of actions to execute, where order is very much
relevant.

The JSON data model (which the YAML data model extends) features only
two types of collections ("structured types" in the
[spec][json-spec]): arrays and objects (records). This pared down data
model is attractive because it is lightweight and easy to understand,
but when encoding to JSON, the intended meaning is often lost. Both
examples above use the same structured type (arrays), but the meaning
is different in each case and that meaning is not conveyed. Without
some out-of-band knowledge like a schema, we don't know whether we're
dealing with a sequence, closed under concatenation, or a set, closed
under set union, but very much not closed under concatenation.

We wish to be able to build `extraGroups` above piecemeal, by
combining snippets of configuration (or *modules*) distant from each
other — in separate files even. The combination order should not
matter. In formal terms, the combination operator should satisfy
associativity, commutativity and idempotence laws, i.e. be the join of
a semilattice. We could try to define our own, including how it
recurses into records while preserving field metadata, how it combines
numbers, strings, and everything else in our configuration. But Nickel
already has the merge operator `(&)`. It satisfies the above laws...
except that it doesn't do what we want. To wit,

```nickel
["wheel"] & ["wheel"]  # => ["wheel"]
[] & ["wheel"]         # => error
["wheel"] & ["audio"]  # => error
```

We want to be able to reuse the existing merge operator to combine
configurations. And for that, we need some form of overloading of the
merge operator on arrays to mean either,

- the default one,
- zipping (for sequences, this in fact subsumes the default),
- or union (for sets),

depending on context. That is, we need some form of late binding of
`(&)` to choose the semilattice structure we want (flat lattice on
arrays, sequences under zipping, sets under union, etc).

### Merging numbers

Linux has a `net.core.rmem_max` sysctl that sets the limit for
a receive socket buffer size. On NixOS, the sysctl is chosen to be the
maximum of the limits requested by all modules in the configuration.
In Nickel syntax, this means we want:

```nickel
# Caddy webserver module
{ config.boot.kernel.sysctl."net.core.rmem_max" | default = 7500000 }

# Transmission BitTorrent module
{ config.boot.kernel.sysctl."net.core.rmem_max" | default = 4194304 }
```

Priorities are not what we want here: they pick the definition that
claims the higher rank, not the one with the larger number. What we
want is the merge `(&)` between the two modules above to be the join
in the semilattice of integers under `max`. That is, we need some form
of overloading of the merge operator on numbers to be the join in
either,

- the flat lattice on numbers (the default),
- the semilattice under `max`,

depending on context. NixOS similarly has the `boolByOr` option type,
which selects the semilattice under boolean OR, when the default is
is to merge by equality.

### Coercions

A set of configuration modules might have many users, as is the case
of NixOS. Over time, the modules' schema is bound to evolve, including
changing the expected types of configurations options. Some option
`foo.bar.baz` might expect a string, but later be changed to expect
a set of strings instead. In this case, we want merging to seamlessly
coerce any encountered bare string to the singleton set, to avoid the
change in expected type being a breaking change for users.

Contract annotations on fields can act as coercions, but since
[RFC005][], they run too late to affect the results of the merge.
Therefore, we need the merge operator to itself apply the coercions.
That is, we sometimes need a nonstandard (but still associative,
commutative and idempotent) merge, depending on context.

### The normalizing contract commutativity problem

Contracts are usually partial identities: when applied, they either
fail or pass through the value as-is (+ possibly some delayed checks).
But some contracts are coercions in the sense above: a retraction of
type `A + B` onto type `A`, which passes through values of `A`
unchanged and converts values of type `B` into values of type `A`.
These contracts are currently called "normalizing contracts" in the
standard library. An example is `std.enum.TagOrString`. As pointed out
in #2715, the problem is that normalizing contracts break
commutativity of merge:

```nickel
nickel> let Schema1 = { protocol | std.enum.TagOrString | [| 'http |] }
nickel> let Schema2 = { protocol | [| 'http |] }
nickel> { protocol = "http" } & Schema1 & Schema2
# => no error
nickel> { protocol = "http" } & Schema2 & Schema1
error: contract broken by the value of `protocol`
       expected an enum
```

The root cause of the problem is that Nickel accumulates contract
annotations on fields across multiple merges in arbitrary order. In
the example, the two orders we get are:

```nickel
| std.enum.TagOrString | [| 'http |] | [| 'http |]
| [| 'http |] | std.enum.TagOrString | [| 'http |]
```

Only contract annotations in the first order succeed, because unlike
validators, normalizing contracts do not in general commute with other
contracts.

We need a new language feature to work around this problem.

## Proposal

We propose the following changes to Nickel:

- a new type `Set a` of sets of elements of type `a`
- a new `coerce` metadata keyword
- a rule for merging custom contracts

### The `Set a` type

Nickel features the two collection types of the JSON data model:
arrays and records. We propose to add a third.

A `Set a` is a primitive type of sets of elements of type `a`. It has
the following API:

```nickel
{
  std.set.empty : forall a. Set a,
  std.set.from_array : forall a. (a -> Dyn) -> Array a -> Set a,
  std.set.to_array : forall a. Set a -> Array a,
  std.set.union : forall a. Set a -> Set a -> Set a,
  std.set.elem : forall a. a -> Set a -> Bool,
}
```

For the serialization of sets as arrays to be independent of the order
in which sets were constructed through successive unions, we need
a canonical representation of sets. This can be achieved given a total
preorder. `std.set.from_array` takes a key function characterizing the
preorder. When taking the union of two sets, elements with identical
keys merge. Elements of the result of `std.set.to_array` are in
ascending order of key values.

A key function must return a scalar or an array of scalars. For each
type, the order is fixed by the language to be the order implied by
the comparison operator for that type found in the standard library
(`std.string.compare` for strings, `std.number.compare` for numbers,
etc). A key function that returns key values of different type is an
error.

Arrays are used to conveniently express compound keys (see the
Kubernetes example below).

Merging sets is defined as set union:

```nickel
s1 & s2 == std.set.union s1 s2
```

Values of type `Set a` serialize to JSON arrays. This choice breaks
the round-tripping property: a Nickel set serialized to JSON and then
deserialized back from JSON is not a set. However, Nickel already has
types that are either unrepresentable in JSON or have the same
representation as other types, like enum tags. To remediate this for
both sets, tags and future types with ambiguous representations, we
propose adding coercions to schemas.

### The `coerce` metadata keyword

Say we had a `std.set.SetBy` contract, that like
`std.enum.TagOrString`, converts into a set the value it is applied
to, if it is not already a set. It uses the supplied key function to
compare elements. We could have,

```nickel
let User = {
  uid
    | Number,
  extraGroups
    | std.set.SetBy std.function.id,
}
in
{ users | { _ : User } }
```

Then we would hope that

```nickel
User & { uid = 1000, extraGroups = ["wheel"] } & { extraGroups = ["audio"] }
```

converts the two arrays to sets, then merges the sets by set union.
However, under lazy propagation of contracts (see [RFC005][]),
contracts are applied after the merge happens — too late. So the above
fails with a merge error on lists.

The proposed `coerce <coercion>` metadata keyword applies a coercion
(defined below) eagerly, so that `(&)` can dispatch on the intended
type. To safeguard associativity, records associate with each field
a *bag of definitions*, just like they associate a bag of contract
annotations today. For each matching field of a pair of records, the
merge pipeline is modified as follows:

1. Merge coercions, if any, pointwise.
2. Take the union of the two bags of definitions.
3. Collect the contract annotations, as today.

A field's value is computed only when the field is forced:

1. Keep the definitions of the highest priority.
2. Apply the field's coercion to each remaining definition.
3. Fold `(&)` over the coerced definitions.
4. Apply the field’s contract annotations to the result, as specified
   in [RFC005][].

In practice, an implementation can avoid recomputation. It can skip
coercing a definition that is already in the coercion’s image, and
cache a field’s value per record. The meaning does not depend on
either optimization.

The [NixOS module system][nixos-modules], which also features
coercions using `coercedTo`, implements the same pipeline: it collects
definitions, filters them by priority, coerces each definition, merges
them, and only then checks them.

Coercions are contracts. Custom contracts are proposed to merged
pointwise, as for functions. On types, merging is done by equality.

Furthermore, a coercion is a contract obeying the following two laws:

1. a coercion is idempotent,
2. the coercion and merge commute:

   ```nickel
   (x & y) | C == (x | C) & (y | C)
   ```

   whenever `x & y` is defined.

The above laws capture the idea that coercions cannot accept two
values but not their merge. Whereas other contracts are partial
identities, coercions can change the values they are applied to. The
trade is that coercions and merging must agree. We'll illustrate this
with a contract that violates the coercion laws stated above.

Consider the coercion that drops all elements in a set except one
element. While it is idempotent, its image is not closed under union.
`{1}` coerces to `{1}` and `{2}` coerces to `{2}` but `{1} ∪ {2} = {1,
2}`, which is not in the image of the coercion. This leads to the
following problem:

```nickel
let num_set = std.set.from_array std.function.id in
let r1 = { a = num_set [1] } in
let r2 = { a = num_set [2] } in
let merged = { a | coerce Drop } & r1 & r2 in
{ a | coerce Drop } & { a = merged.a } == merged
# => false: merged.a is {1, 2}, but fed back in it coerces to {1}
```

The coercion laws need not apply to contracts in field annotations.
Those apply after any merging, so they are free to accept a set of
values that is not closed under union. Rejecting the union of two
acceptable values is in fact often the point of validation:

```nickel
let Pair = std.contract.from_predicate (fun s => std.set.length s == 2) in
let p = { a = num_set [1, 2] } in
let q = { a = num_set [2, 3] } in
{ a | Pair } & p       # => { a = {1, 2} }
{ a | Pair } & q       # => { a = {2, 3} }
{ a | Pair } & p & q   # => error: {1, 2, 3} has three elements
```

Going back to the use case at the beginning of this section, we add
the `coerce` metadata keyword to the schema:

```nickel
let User = {
  uid
    | Number,
  extraGroups
    | coerce std.set.SetBy std.function.id,
}
in
{ users | { _ : User } }
```

And now, the following works as expected:

```nickel
nickel> User & { uid = 1000, extraGroups = ["wheel"] } & { extraGroups = ["audio"] }
# => { uid | Number = 1000, extraGroups = %set/from_array% std.string.compare ["audio", "wheel"] }
```

Likewise, #2715 is fixed by prepending the `coerce` keyword to
`std.enum.TagOrString`.

### Example

In this example, we work with sets of records with compound primary
keys.

In Kubernetes, a container can expose a set of ports:

```nickel
let Port = {
  containerPort | Number,
  protocol | String | default = "TCP",
  name | String | optional,
  ..
}
in

let Container = {
  name | String,
  image | String | optional,
  ports
    | Set Port
    | coerce std.set.SetBy (fun x => [x.containerPort, x.protocol])
    | optional,
  ..
}
```

Ports have a compound key, since TCP and UDP are separate port spaces,
so there can be both a `53/tcp` and a `53/udp` port.

One or more containers can be grouped into pods, which have some
metadata, optionally including a list of finalizers (and much else
besides):

```nickel
let Pod = {
  apiVersion | String,
  kind | String,
  metadata = {
    name | String,
    finalizers
      | Set String
      | coerce std.set.SetBy std.function.id
      | default = [],
    ..
  },
  spec = {
    containers
      | Set Container
      | std.set.NonEmpty
      | coerce std.set.SetBy (fun x => x.name),
    ..
  },
  ..
}
in
{ pods | Set Pod | coerce std.set.SetBy (fun x => x.metadata.name) }
```

We identify pods using their "name" metadata.

To recapitulate, the above example demonstrates sets of atoms, sets
with compound keys, and fields in nested records as keys:

```nickel
{
  finalizers | Set String | coerce std.set.SetBy std.function.id,
  containers | Set Container | coerce std.set.SetBy (fun x => x.name),
  ports      | Set Port
             | coerce std.set.SetBy (fun x => [x.containerPort, x.protocol]),
  pods       | Set Pod | coerce std.set.SetBy (fun x => x.metadata.name),
}
```

With `coerce` metadata annotations, we address the strongest
motivation for custom merge functions provided in [RFC001][], namely
merging arrays through some form of set union or list concatenation.
It also provides enough to implement most of the NixOS module system
in Nickel.

Unlike in [RFC001][], merging is still driven by the shape of values
alone. So merging in a different semilattice of numbers like `max`, as
in the `net.core.rmem_max` sysctl use case in the motivation, is not
supported. We consider this use case rare enough to set it aside.
Likewise, a user cannot ask for the semilattice of arrays under
zipping, since for arrays merging is already defined to be the join of
the flat lattice (arrays merge if and only if they are equal). We
consider merging by position elements in sequences defined
independently to be an anti pattern anyway, and have not found
convincing use cases for it in the wild.

If controlling the join semilattice of merges is necessary, we can
always introduce custom merge functions later. The main cost of custom
merge functions is that they require the bag of definitions introduced
here. But if the `coerce` metadata keyword is added, then carrying
this bag around is a sunk cost.

## Alternatives

### Coercions as their own type

The literature usually studies contracts as partial identities.
Normalizing contracts do not fit in that framework. At the same time,
normalizing contracts are useful in practice. A normalizing contract
is morally a lawful function (a retraction). We could consider
breaking them out into their own type of coercions, such that
coercions and contracts never mix. We would allow coercions only in
`coerce` metadata and contracts only in field contract annotations.
This makes sense because as pointed out in [RFC005][], partial
identities ought to be applied *after* all merges, to avoid breaking
associativity. Coercions need to run *before* all merges, to avoid
breaking commutativity. Mixing both contract characteristics is
unlawful and causes its own problems as seen above.

However, doing so would have these downsides:

1. Contracts for collections are already parameterized and already
   know how to distribute the parameter contract over their
   constituent parts. For example, `Array TagOrString` applied to
   `["a", "b"]` already does the right thing. It is equivalent to
   `["a" | TagOrString, "b" | TagOrString]`, i.e. `['a, 'b]`. For
   coercions, we would likewise need an array combinator, with the
   same behaviour. We would not be able to call it `Array`, because
   that is already the name of the contract for the `Array T` type.
2. There would be no way of applying coercions inside expressions,
   since there is nowhere to hang a coercion annotation. We would need
   either,
   1. new syntax, or
   2. a new primitive, say `%coercion/apply%`, for applying coercions
      inside expressions, or
   3. coercions as mere functions, which can be applied like any
      function.
3. Segregating coercions from contracts would be a breaking change.
   Existing normalizing contracts would need to be phased out over
   time.

### Custom merge functions

Custom merge functions, as envisioned in [RFC001][], are a more
general solution than adding the `coerce` keyword. With custom merge
functions, the user can choose the semantics of the merge operator
`(&)` as the join of any semilattice. For example, numbers under `max`
form a semilattice. However, with great power comes great
responsibility. We would be leaving to the user the burden of ensuring
that their custom merge functions really satisfy the semilattice laws.
In this proposal, the laws hold under certain assumptions about
coercions stated above, because the behaviour of the merge operator
depends only on the shape of the field values, as it does in Nickel
today.

Under this proposal, the `net.core.rmem_max` sysctl use case would
work as follows:

```nickel
{
  _net.core.rmem_max
    | not_exported
    | coerce std.set.SetOf Number std.number.compare,
  net.core.rmem_max =
    _net.core.rmem_max
    |> std.set.to_array
    |> std.array.reduce_left std.number.max,
} & {
  _net.core.rmem_max = 7500000,
}
```

This construction exploits the fact that finite sets, under union,
form the free semilattice, which doesn't lose information.

### Implicit set order

In this proposal, the user provides an order of their choosing when
creating a set. We could instead have the evaluator infer an implicit
order. This has the advantage of brevity. We wouldn't need to pass
a key function to set constructors or to coercions. It has the
disadvantage that,

1. comparing elements structurally may be less performant, when
   a primary key of one or two fields is nearly always readily
   available.
2. Worse, absent fast paths like pointer equality, structural
   comparison has to force evaluation of everything in a record. So
   it's not as lazy as the user might like.
3. Not everything one might find in a record is comparable (functions,
   contracts, etc).
4. If based on a hash, the implicit order becomes implementation
   defined, so canonical representations change across Nickel
   versions.

In this proposal, we opt for an explicit order. An implicit order can
always be built on top by users who prefer that, say by building
domain-specific coercions, or as follows:

```nickel
let SetByDefault = std.set.SetBy std.to_string
```

## Appendix

### Current workarounds

Users today cannot merge arrays that they think of as sets, but
a workaround is to reach for records instead. Given a homogeneous set
of records, a user can choose one or more fields of the records to be
the primary key by which each element of the set is identified. Then
the set can be modelled as a record where the field names are the
key values. So instead of,

```nickel
let some_fruits = [
  { name = "apple", colour = "red" },
  { name = "banana", colour = "yellow" },
  { name = "cherry", colour = "red" },
]
let more_fruits = [
  { name = "strawberry", colour = "red" },
]
```

choosing `name` as the key, we have,

```nickel
let some_fruits_record = {
  apple = { name = "apple", colour = "red" },
  banana = { name = "banana", colour = "yellow" },
  cherry = { name = "cherry", colour = "red" },
}
let more_fruits_record = {
  strawberry = { name = "strawberry", colour = "red" },
}
```

This set can be merged with other sets the way we expect:

```nickel
some_fruits_record & more_fruits_record
```

However, this solution has several drawbacks:

- denoting a set is not as immediate as just writing down a list
  literal,
- importing a record from a YAML or JSON file with a set nested deeply
  requires some list-to-record preprocessing (though YAML has limited
  support for sets via the `!!set` tag),
- some record-to-list post processing is required to avoid the key
  values polluting the output at serialization.

This is all the more frustrating given that for all the arrays out
there representing sequences, there are likely at least as many
representing sets. Yet sequences have a natural and direct notation in
Nickel, but sets do not.

[json-spec]: https://www.rfc-editor.org/info/rfc8259/
[nixos-modules]: https://nix.dev/tutorials/module-system/index.html
[rfc001]: ./001-overriding.md
[rfc005]: ./005-metadata-rework.md
