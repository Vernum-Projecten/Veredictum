- Two development-only AQL cases select an earlier version of a COMPOSITION
  through a standard predicate on `VERSION`, one on its `uid` and one on its
  commit change type. Each asserts that the first version's row is returned
  and asserts no row count, since the development text does not say whether
  the predicate replaces the latest-only default or combines with it.
