- The container image carries one copy of the ITS-XML 2.0.0 schemas,
  `specs/its-xml-schemas/its-xml-2.0.0-nsv2/`. The byte-identical
  `specs/its-xml-schemas/components/` tree is gone, and
  `scripts/vendor/its-xml-schemas.sh` refreshes both XSD lineages from their
  pinned commits.
