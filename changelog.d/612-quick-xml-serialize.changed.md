- The `quick-xml` dependency no longer enables its `serialize` feature, which
  nothing in the crate used, so serde support for it leaves the build.
