- The `xml-namespace` option now asks only whether a service also accepts
  canonical XML in the TRIAL Release-2.0.0 namespace
  `http://schemas.openehr.org/v2`. Acceptance of the STABLE Release-1.0.2
  namespace `http://schemas.openehr.org/v1` is BASE, citing upstream report
  #759. The `xml-namespace-v2` arm gains its first document, development-only:
  a v2 ADL 1.4 operational template with the AM 1.4 ordinal shape is accepted,
  and one whose ordinal list entries keep `xsi:type="DV_ORDINAL"` is refused
  with 400. The development citations that grounded v1 template uploads and
  the XML COMPOSITION write responses on the v2 schemas are removed from
  `template_adl14` and `composition_write_xml_accept`.
