{
  name = "missing-index-dep",
  description = "A package depending on an index package that doesn't exist",
  version = "0.1.0",
  authors = ["Joe"],
  minimal_nickel_version = "1.9.0",
  dependencies = {
    missing = 'Index { package = "github:example/does-not-exist", version = "0.1.0" },
  },
} | std.package.Manifest
