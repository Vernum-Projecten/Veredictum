- The catalogue declares its reading structure in `catalogue/chapters.yaml`:
  chapters in order, the capability folders of each, every folder's BASE
  sections, and each option's theme and tier (`recommended`, `options` or
  `spec-boundary`). A document no option arm names is BASE and names its
  section with `x-veredictum-section` on its `info`. `validate` refuses a
  folder in no chapter or in two, a BASE document with no section or an
  unknown one, a section no document uses, an OPTION document with a section,
  an option with no theme and tier, and a spec-boundary option whose citation
  names no report, each under the new `arazzo-structure` gate. The
  structure is published as `schemas/arazzo-chapters.schema.json`.
- `catalogue/reports.yaml` records the title and state of every upstream
  report a citation names, published as `schemas/arazzo-reports.schema.json`.
  `validate` refuses a cited report the file lacks and an entry no citation
  names, and `scripts/checks/report-refs.sh` holds each entry to the tracker.
- A catalogue run's `results.json` carries an `outline`: the chapters, every
  option with the arms the statement selected, the recorded reports, and for
  each case its chapter, folder, BASE section or option arms, a TRIAL mark,
  the reports it rests on and its template formalism.
