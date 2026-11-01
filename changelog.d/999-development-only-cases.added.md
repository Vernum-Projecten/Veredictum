- A case can rest on development text alone: a citation whose `source` sits
  under `specs/openehr-development/` is checked against the pinned snapshot by
  `validate` under both profiles, carries no `development` wording, and makes
  its workflow a development-only case. A released run skips it with the new
  skip reason `development-only`, whose detail names the citation, and no
  released coverage, floor, claim or tally counts it; a development run judges
  it. `validate --profile development` adds a line counting the
  development-only workflows and the released cases whose cited text the
  snapshot rewords or removes. Under the development profile the ITS-XML 2.0.0
  schemas now read from the snapshot's `components/` tree.
