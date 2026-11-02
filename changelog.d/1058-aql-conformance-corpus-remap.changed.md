- The aql-conformance corpus import moves to commit `3ecd250`. All 555 of its
  case ids are in `catalogue/id-map.yaml`. Of the 60 SELECT cases the corpus
  added, 3 are carried by new `aqlc_select` workflows (a quoted `'True'` and a
  quoted `'3e102'` stay String literals, and a quoted date/time literal
  projects as the instant it writes), 4 fold onto the existing literal
  workflows and 53 are not carried, each with the reason or upstream report
  that blocks it.
