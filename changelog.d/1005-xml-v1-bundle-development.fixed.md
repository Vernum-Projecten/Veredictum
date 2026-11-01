- The development profile reads the ITS-XML Release-1.0.2 schemas
  (`specs/its-xml-schemas/its-xml-1.0.2-nsv1/`) in place, since the
  snapshot's ITS-XML manifest names that release as its STABLE `ITS-XML-v1`
  document. Citations of the v1 schemas now resolve under the development
  profile, where they were skipped as cited-text-absent, and the development
  wordings that read v1 XML responses against the v2 schemas are removed from
  `composition_read_xml`, `composition_versioned_read_xml` and
  `directory_read_xml`.
