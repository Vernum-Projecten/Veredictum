- Two development-only cases refuse an ADL 2 template upload with an empty
  body and one whose body does not parse, each with 400, controlled by the
  at-coded upload. A development run judges both whichever arm of the
  `adl2-id-coded` option a statement selects; a released run skips them as
  `development-only`, where the id-coded originals run as before.
